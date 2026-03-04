use defmt::info;
use embassy_stm32::adc::{Adc, AdcConfig, AnyAdcChannel, SampleTime};
use embassy_stm32::peripherals::{ADC1, ADC2};
use embassy_stm32::Peri;
use embassy_time::{Duration, Timer};

use crate::config::EXPRESSION_POLL_HZ;
use crate::midi::{MidiEvent, MidiMessage, MidiPeripheral, MidiSender, MidiSource};

// ---------------------------------------------------------------------------
// Per-channel config (public) — one entry per TRS expression jack
// ---------------------------------------------------------------------------

/// Configuration for one TRS expression input jack.
///
/// `ADC` is the ADC peripheral type (`ADC1` or `ADC2`).
/// Channels are type-erased via [`AnyAdcChannel`] so that multiple jacks on
/// the same ADC can be collected into a fixed-size array without per-pin
/// type parameters leaking into [`ExpressionConfig`].
///
/// Use [`embassy_stm32::adc::AdcChannel::degrade_adc`] on any ADC-capable pin
/// to obtain an [`AnyAdcChannel`]:
///
/// ```ignore
/// ExpressionChannelConfig { v_tip: p.PA0.degrade_adc(), v_sleeve: p.PA1.degrade_adc(), cc: 11 }
/// ```
pub struct ExpressionChannelConfig<ADC: embassy_stm32::adc::Instance> {
    /// Expression signal — wiper of the potentiometer (V_tip).
    pub v_tip:    AnyAdcChannel<'static, ADC>,
    /// Reference voltage — plug presence / supply calibration (V_sleeve).
    pub v_sleeve: AnyAdcChannel<'static, ADC>,
    /// MIDI CC number to emit when this jack's value changes.
    pub cc:       u8,
}

// ---------------------------------------------------------------------------
// Driver config (public)
// ---------------------------------------------------------------------------

/// Construction parameters for [`ExpressionDriver`].
///
/// `N1` is the number of jacks wired to ADC1; `N2` to ADC2.
///
/// Example — default 2 + 2 layout:
/// ```ignore
/// ExpressionConfig {
///     adc1: p.ADC1,
///     adc1_channels: [
///         ExpressionChannelConfig { v_tip: p.PA0.degrade_adc(), v_sleeve: p.PA1.degrade_adc(), cc: 11 },
///         ExpressionChannelConfig { v_tip: p.PA2.degrade_adc(), v_sleeve: p.PA3.degrade_adc(), cc: 1  },
///     ],
///     adc2: p.ADC2,
///     adc2_channels: [
///         ExpressionChannelConfig { v_tip: p.PA4.degrade_adc(), v_sleeve: p.PA5.degrade_adc(), cc: 7  },
///         ExpressionChannelConfig { v_tip: p.PA6.degrade_adc(), v_sleeve: p.PA7.degrade_adc(), cc: 74 },
///     ],
///     midi_channel: 0,
/// }
/// ```
pub struct ExpressionConfig<const N1: usize, const N2: usize> {
    pub adc1:          Peri<'static, ADC1>,
    pub adc1_channels: [ExpressionChannelConfig<ADC1>; N1],
    pub adc2:          Peri<'static, ADC2>,
    pub adc2_channels: [ExpressionChannelConfig<ADC2>; N2],
    /// MIDI channel for all CC messages from this driver (0-indexed, 0 = channel 1).
    pub midi_channel:  u8,
}

// ---------------------------------------------------------------------------
// Internal per-channel state — defined via macro to sidestep the SampleTime
// associated-type issue that arises with a generic ADC type parameter.
// Each expansion produces a concrete struct for one ADC peripheral.
// ---------------------------------------------------------------------------

macro_rules! define_channel_state {
    ($name:ident, $adc:ty) => {
        struct $name {
            v_tip:    AnyAdcChannel<'static, $adc>,
            v_sleeve: AnyAdcChannel<'static, $adc>,
            cc:       u8,
            last_cc:  u8,
        }

        impl $name {
            /// Sample V_tip and V_sleeve, returning a new CC value (0–127) only
            /// when the value has changed (simple equality check).
            ///
            /// Normalises by `v_sleeve` to compensate for supply variation.
            fn sample(&mut self, adc: &mut Adc<'static, $adc>) -> Option<u8> {
                let tip    = adc.blocking_read(&mut self.v_tip,    SampleTime::CYCLES2_5) as u32;
                let sleeve = adc.blocking_read(&mut self.v_sleeve, SampleTime::CYCLES2_5) as u32;

                let raw = if sleeve > 0 {
                    (tip * 4095 / sleeve).min(4095)
                } else {
                    tip
                };

                let cc = (raw * 127 / 4095) as u8;
                if cc != self.last_cc {
                    self.last_cc = cc;
                    Some(cc)
                } else {
                    None
                }
            }
        }
    };
}

define_channel_state!(ChannelAdc1, ADC1);
define_channel_state!(ChannelAdc2, ADC2);

// ---------------------------------------------------------------------------
// Driver struct
// ---------------------------------------------------------------------------

/// Expression pedal driver.
///
/// Owns both ADC peripherals and all TRS expression input jacks.
/// `N1` jacks are serviced by ADC1; `N2` jacks by ADC2.
/// All channels are polled sequentially inside a single embassy task at
/// [`EXPRESSION_POLL_HZ`].
pub struct ExpressionDriver<const N1: usize, const N2: usize> {
    adc1:         Adc<'static, ADC1>,
    adc1_chs:     [ChannelAdc1; N1],
    adc2:         Adc<'static, ADC2>,
    adc2_chs:     [ChannelAdc2; N2],
    midi_channel: u8,
}

impl<const N1: usize, const N2: usize> ExpressionDriver<N1, N2> {
    pub fn new(config: ExpressionConfig<N1, N2>) -> Self {
        // [T; N]::map() consumes the array and transforms each element in place.
        let adc1_chs = config.adc1_channels.map(|ch| ChannelAdc1 {
            v_tip:    ch.v_tip,
            v_sleeve: ch.v_sleeve,
            cc:       ch.cc,
            last_cc:  0xFF,
        });
        let adc2_chs = config.adc2_channels.map(|ch| ChannelAdc2 {
            v_tip:    ch.v_tip,
            v_sleeve: ch.v_sleeve,
            cc:       ch.cc,
            last_cc:  0xFF,
        });
        Self {
            adc1: Adc::new(config.adc1, AdcConfig::default()),
            adc1_chs,
            adc2: Adc::new(config.adc2, AdcConfig::default()),
            adc2_chs,
            midi_channel: config.midi_channel,
        }
    }
}

// ---------------------------------------------------------------------------
// MidiSource impl
// ---------------------------------------------------------------------------

impl<const N1: usize, const N2: usize> MidiSource for ExpressionDriver<N1, N2> {
    async fn run(mut self, to_bus: MidiSender<'static>) {
        info!("Expression task started ({} + {} channels)", N1, N2);

        let interval = Duration::from_hz(EXPRESSION_POLL_HZ);

        loop {
            let adc1 = &mut self.adc1;
            for (i, ch) in self.adc1_chs.iter_mut().enumerate() {
                if let Some(value) = ch.sample(adc1) {
                    emit_cc(&to_bus, i, self.midi_channel, ch.cc, value);
                }
            }

            let adc2 = &mut self.adc2;
            for (i, ch) in self.adc2_chs.iter_mut().enumerate() {
                if let Some(value) = ch.sample(adc2) {
                    emit_cc(&to_bus, N1 + i, self.midi_channel, ch.cc, value);
                }
            }

            Timer::after(interval).await;
        }
    }
}

fn emit_cc(to_bus: &MidiSender<'static>, index: usize, midi_ch: u8, cc: u8, value: u8) {
    let event = MidiEvent::new(
        MidiPeripheral::Expression(index as u8),
        MidiMessage::ControlChange { channel: midi_ch, control: cc, value },
    );
    if to_bus.try_send(event).is_err() {
        defmt::warn!("Expression ch{}: bus full, event dropped", index);
    }
}

#[embassy_executor::task]
pub async fn expression_task(
    driver: ExpressionDriver<2, 2>,
    to_bus: MidiSender<'static>,
) {
    use MidiSource;
    driver.run(to_bus).await;
}
