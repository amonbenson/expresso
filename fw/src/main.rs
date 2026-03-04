#![no_std]
#![no_main]

mod config;
mod din_midi;
mod expression;
mod midi;
mod router;
mod usb_midi;

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::{Config, bind_interrupts, peripherals, usart, usb};
use expression::ExpressionDriver;
use midi::{MidiEventChannel, MidiSender};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USB_LP  => usb::InterruptHandler<peripherals::USB>;
    USART1  => usart::InterruptHandler<peripherals::USART1>;
});

static MIDI_BUS: MidiEventChannel = MidiEventChannel::new();
static TO_USB:   MidiEventChannel = MidiEventChannel::new();
static TO_DIN:   MidiEventChannel = MidiEventChannel::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Config::default());

    info!("Expresso firmware starting");

    // Router: consumes MIDI_BUS, dispatches to TO_USB / TO_DIN.
    spawner
        .spawn(router::task(MIDI_BUS.receiver(), TO_USB.sender(), TO_DIN.sender()))
        .unwrap();

    // USB MIDI: bidirectional bridge between the USB host and the MIDI bus.
    let usb_midi = usb_midi::UsbMidi::new(usb_midi::UsbMidiConfig {
        usb: p.USB,
        dp:  p.PA12,
        dm:  p.PA11,
    });
    spawner
        .spawn(usb_midi::task(usb_midi, TO_USB.receiver(), MIDI_BUS.sender()))
        .unwrap();

    // DIN MIDI: bidirectional bridge between the 5-pin DIN jack and the MIDI bus.
    let din_midi = din_midi::DinMidi::new(din_midi::DinMidiConfig {
        usart:  p.USART1,
        tx_pin: p.PA9,
        rx_pin: p.PA10,
        tx_dma: p.DMA1_CH4,
        rx_dma: p.DMA1_CH5,
    });
    spawner
        .spawn(din_midi::task(din_midi, TO_DIN.receiver(), MIDI_BUS.sender()))
        .unwrap();

    // Expression pedals: producer-only, samples all four TRS jacks via ADC.
    // Jack pin mapping (from expresso.ioc):
    //   Jack 0 — PA0 (V_tip), PA1 (V_sleeve) → ADC1,  CC 11 (Expression)
    //   Jack 1 — PA2 (V_tip), PA3 (V_sleeve) → ADC1,  CC  1 (Modulation)
    //   Jack 2 — PA4 (V_tip), PA5 (V_sleeve) → ADC2,  CC  7 (Volume)
    //   Jack 3 — PA6 (V_tip), PA7 (V_sleeve) → ADC2,  CC 74 (Brightness)
    use embassy_stm32::adc::AdcChannel;
    let expression = ExpressionDriver::new(expression::ExpressionConfig {
        adc1: p.ADC1,
        adc2: p.ADC2,
        adc1_channels: [
            expression::ExpressionChannelConfig { v_tip: p.PA0.degrade_adc(), v_sleeve: p.PA1.degrade_adc(), cc: 11 },
            expression::ExpressionChannelConfig { v_tip: p.PA2.degrade_adc(), v_sleeve: p.PA3.degrade_adc(), cc: 1  },
        ],
        adc2_channels: [
            expression::ExpressionChannelConfig { v_tip: p.PA4.degrade_adc(), v_sleeve: p.PA5.degrade_adc(), cc: 7  },
            expression::ExpressionChannelConfig { v_tip: p.PA6.degrade_adc(), v_sleeve: p.PA7.degrade_adc(), cc: 74 },
        ],
        midi_channel: 0,
    });
    spawner.spawn(expression::expression_task(expression, MIDI_BUS.sender())).unwrap();
}
