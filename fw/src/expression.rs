use embassy_stm32::adc::{Adc, AnyAdcChannel, SampleTime};
use embassy_stm32::peripherals::{ADC1, ADC2};
use embassy_time::{Duration, Timer};
use expresso::component::Component;
use expresso::expression::group::ExpressionGroup;
use expresso::midi::types::MidiEndpoint;
use expresso::midi::{MidiMessage, MidiMessageSink};
use expresso::settings::Settings;

use crate::{MsgSender, config::EXPRESSION_POLL_HZ};

const VREF: f32 = 3.3;
const ADC_MAX: f32 = 4095.0;

// Forwards expression CC messages to the EXP_TO_ROUTER channel.
struct ExpSink(MsgSender);

impl MidiMessageSink for ExpSink {
    fn emit(&mut self, message: MidiMessage<'_>, _target: Option<MidiEndpoint>) {
        if let Some(msg) = crate::to_static(message) {
            let _ = self.0.try_send(msg);
        }
    }
}

#[embassy_executor::task]
pub async fn task(
    mut adc1: Adc<'static, ADC1>,
    mut adc2: Adc<'static, ADC2>,
    mut adc1_channels: [(AnyAdcChannel<'static, ADC1>, AnyAdcChannel<'static, ADC1>); 2],
    mut adc2_channels: [(AnyAdcChannel<'static, ADC2>, AnyAdcChannel<'static, ADC2>); 2],
    to_router: MsgSender,
) {
    let mut group = ExpressionGroup::<4>::new();
    let mut settings = Settings::<4>::default();
    let mut sink = ExpSink(to_router);
    let interval = Duration::from_hz(EXPRESSION_POLL_HZ);

    loop {
        let inputs = [
            read_pair_adc1(&mut adc1, &mut adc1_channels[0]),
            read_pair_adc1(&mut adc1, &mut adc1_channels[1]),
            read_pair_adc2(&mut adc2, &mut adc2_channels[0]),
            read_pair_adc2(&mut adc2, &mut adc2_channels[1]),
        ];

        let _ = group.process(inputs, &mut sink, &mut settings);

        Timer::after(interval).await;
    }
}

fn read_pair_adc1(
    adc: &mut Adc<'static, ADC1>,
    channels: &mut (AnyAdcChannel<'static, ADC1>, AnyAdcChannel<'static, ADC1>),
) -> (f32, f32) {
    let ring = raw_to_voltage(adc.blocking_read(&mut channels.0, SampleTime::CYCLES2_5.into()));
    let sleeve = raw_to_voltage(adc.blocking_read(&mut channels.1, SampleTime::CYCLES2_5.into()));
    (ring, sleeve)
}

fn read_pair_adc2(
    adc: &mut Adc<'static, ADC2>,
    channels: &mut (AnyAdcChannel<'static, ADC2>, AnyAdcChannel<'static, ADC2>),
) -> (f32, f32) {
    let ring = raw_to_voltage(adc.blocking_read(&mut channels.0, SampleTime::CYCLES2_5.into()));
    let sleeve = raw_to_voltage(adc.blocking_read(&mut channels.1, SampleTime::CYCLES2_5.into()));
    (ring, sleeve)
}

fn raw_to_voltage(raw: u16) -> f32 {
    (raw as f32) * VREF / ADC_MAX
}
