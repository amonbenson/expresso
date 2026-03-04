use defmt::info;
use embassy_stm32::adc::{Adc, SampleTime};
use embassy_stm32::{peripherals, Peri};
use embassy_time::{Duration, Timer};

use crate::config::EXPRESSION_POLL_HZ;
use crate::midi::MidiSender;

#[embassy_executor::task]
pub async fn task(
    adc1: Peri<'static, peripherals::ADC1>,
    adc1_ch0_sig: Peri<'static, peripherals::PA0>,
    adc1_ch0_ref: Peri<'static, peripherals::PA1>,
    adc1_ch1_sig: Peri<'static, peripherals::PA2>,
    adc1_ch1_ref: Peri<'static, peripherals::PA3>,
    adc2: Peri<'static, peripherals::ADC2>,
    adc2_ch2_sig: Peri<'static, peripherals::PA4>,
    adc2_ch2_ref: Peri<'static, peripherals::PA5>,
    adc2_ch3_sig: Peri<'static, peripherals::PA6>,
    adc2_ch3_ref: Peri<'static, peripherals::PA7>,
    to_bus: MidiSender<'static>,
) {
    let _ = (
        adc1, adc1_ch0_sig, adc1_ch0_ref, adc1_ch1_sig, adc1_ch1_ref,
        adc2, adc2_ch2_sig, adc2_ch2_ref, adc2_ch3_sig, adc2_ch3_ref,
        to_bus,
    );

    info!("Expression task started");

    // TODO: Initialise ADC1 and ADC2, then poll at EXPRESSION_POLL_HZ.
    //       Only emit a MidiEvent when the CC value actually changes (hysteresis
    //       or simple equality check) to avoid saturating the event bus.
    //       Rough structure:
    //
    //   let mut adc1 = Adc::new(adc1);
    //   let mut adc2 = Adc::new(adc2);
    //   let mut last_cc = [0u8; 4];
    //   let interval = Duration::from_hz(EXPRESSION_POLL_HZ);
    //
    //   loop {
    //       // Sample channel 0: ADC1, PA0 / PA1
    //       let sig0 = adc1.blocking_read(&mut adc1_ch0_sig);
    //       let ref0 = adc1.blocking_read(&mut adc1_ch0_ref);
    //       emit_if_changed(0, sig0, ref0, &mut last_cc, &to_bus).await;
    //       // ... repeat for channels 1–3 ...
    //       Timer::after(interval).await;
    //   }

    loop {
        Timer::after_millis(1000 / EXPRESSION_POLL_HZ).await;
    }
}

pub struct ExpressionDevice<ADC, PIN_VR, PIN_VS>
where
    ADC: embassy_stm32::adc::Instance,
    PIN_VR: embassy_stm32::adc::AdcChannel<ADC>,
    PIN_VS: embassy_stm32::adc::AdcChannel<ADC>,
{
    adc: Adc<'static, ADC>,
    pin_vr: PIN_VR,
    pin_vs: PIN_VS,
}

impl<ADC, PIN_VR, PIN_VS> ExpressionDevice<ADC, PIN_VR, PIN_VS>
where
    ADC: embassy_stm32::adc::Instance,
    PIN_VR: embassy_stm32::adc::AdcChannel<ADC>,
    PIN_VS: embassy_stm32::adc::AdcChannel<ADC>,
{
    pub fn new(adc: Adc<'static, ADC>, pin_vr: PIN_VR, pin_vs: PIN_VS) -> Self {
        Self { adc, pin_vr, pin_vs }
    }

    pub async fn task(expression_device: &mut ExpressionDevice<ADC, PIN_VR, PIN_VS>, to_bus: MidiSender<'static>) {
        let interval = Duration::from_hz(EXPRESSION_POLL_HZ);

        loop {
            let xr = expression_device.adc.blocking_read(&mut expression_device.pin_vr, SampleTime::CYCLES2_5);
            let xs = expression_device.adc.blocking_read(&mut expression_device.pin_vs, SampleTime::CYCLES2_5);
            Timer::after(interval).await;
        }
    }
}
