use embassy_stm32::adc::{Adc, AnyAdcChannel, BasicAdcRegs, SampleTime};
use embassy_stm32::peripherals::{ADC1, ADC2};
use embassy_time::{Duration, Timer};
use expresso::expression::ExpressionGroup;
use expresso::midi::{MidiEndpoint, MidiMessage, MidiProcessor, MidiSink};

use crate::{InMsgSender, SettingsMutex, config::EXPRESSION_POLL_HZ};

const VREF: f32 = 3.3;
const ADC_MAX: f32 = 4095.0;

// Forwards expression CC messages to the TO_ROUTER channel, tagged as Expression.
struct ExpSink(InMsgSender);

impl MidiSink for ExpSink {
    fn emit(&mut self, message: MidiMessage, _target: Option<MidiEndpoint>) {
        let _ = self.0.try_send((message, MidiEndpoint::Expression));
    }
}

#[embassy_executor::task]
pub async fn task(
    mut adc1: Adc<'static, ADC1>,
    mut adc2: Adc<'static, ADC2>,
    mut adc1_channels: [(AnyAdcChannel<'static, ADC1>, AnyAdcChannel<'static, ADC1>); 2],
    mut adc2_channels: [(AnyAdcChannel<'static, ADC2>, AnyAdcChannel<'static, ADC2>); 2],
    to_router: InMsgSender,
    settings: &'static SettingsMutex,
) {
    let mut group = ExpressionGroup::new();
    let mut sink = ExpSink(to_router);
    let interval = Duration::from_hz(EXPRESSION_POLL_HZ);

    loop {
        let inputs = [
            read_adc_pair::<ADC1>(&mut adc1, &mut adc1_channels[0]),
            read_adc_pair::<ADC1>(&mut adc1, &mut adc1_channels[1]),
            read_adc_pair::<ADC2>(&mut adc2, &mut adc2_channels[0]),
            read_adc_pair::<ADC2>(&mut adc2, &mut adc2_channels[1]),
        ];

        settings.lock(|s| {
            let _ = group.process(inputs, &mut sink, &mut s.borrow_mut());
        });

        Timer::after(interval).await;
    }
}

fn read_adc_pair<ADC>(
    adc: &mut Adc<'static, ADC>,
    channels: &mut (AnyAdcChannel<'static, ADC>, AnyAdcChannel<'static, ADC>),
) -> (f32, f32)
where
    ADC: embassy_stm32::adc::Instance,
    <ADC::Regs as BasicAdcRegs>::SampleTime: From<SampleTime>,
{
    let ring = raw_to_voltage(adc.blocking_read(&mut channels.0, SampleTime::CYCLES2_5.into()));
    let sleeve = raw_to_voltage(adc.blocking_read(&mut channels.1, SampleTime::CYCLES2_5.into()));
    (ring, sleeve)
}

fn raw_to_voltage(raw: u16) -> f32 {
    (raw as f32) * VREF / ADC_MAX
}
