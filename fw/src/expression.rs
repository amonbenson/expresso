use embassy_stm32::adc::{Adc, AnyAdcChannel, BasicAdcRegs, Instance as AdcInstance, SampleTime};
use embassy_stm32::dac::Channel;
use embassy_stm32::peripherals::{ADC1, ADC2};
use embassy_time::{Duration, Timer};

use crate::config::{EXPRESSION_NUM_CHANNELS, EXPRESSION_POLL_HZ};
use crate::midi::{MidiMessageReceiver, MidiMessageSender};
use crate::settings::{ChannelSettings, DeviceSettings};

type StaticAdcChannel<T: AdcInstance> = AnyAdcChannel<'static, T>;
type StaticAdc<T: AdcInstance> = Adc<'static, T>;
type StaticAdc1 = Adc<'static, ADC1>;
type StaticAdc2 = Adc<'static, ADC2>;

pub struct AdcInput<T: AdcInstance> {
    channel: StaticAdcChannel<T>,
}

impl<T> AdcInput<T>
where
    T: AdcInstance,
    <T::Regs as BasicAdcRegs>::SampleTime: From<SampleTime>,
{
    pub fn new(channel: StaticAdcChannel<T>) -> Self {
        Self { channel }
    }

    pub fn read_raw(&mut self, adc: &mut StaticAdc<T>) -> u16 {
        adc.blocking_read(&mut self.channel, SampleTime::CYCLES2_5.into())
    }

    pub fn read_voltage(&mut self, adc: &mut StaticAdc<T>) -> f32 {
        self.read_raw(adc).into()
    }
}

pub struct ExpressionChannel<T: AdcInstance> {
    settings: ChannelSettings,
    v_ring: AdcInput<T>,
    v_sleeve: AdcInput<T>,
}

impl<T> ExpressionChannel<T>
where
    T: AdcInstance,
    <T::Regs as BasicAdcRegs>::SampleTime: From<SampleTime>,
{
    pub fn new(v_ring_channel: StaticAdcChannel<T>, v_sleeve_channel: StaticAdcChannel<T>) -> Self {
        Self {
            settings: ChannelSettings::default(),
            v_ring: AdcInput::new(v_ring_channel),
            v_sleeve: AdcInput::new(v_sleeve_channel),
        }
    }

    fn calculate_resistance(&self, v_ring: f32, v_sleeve: f32) -> (f32, f32) {
        let v_cc: f32 = 3.3;
        let r_14: f32 = 10.0;
        let r_23: f32 = 100.0;

        // calculate I using formula R_4 = V_Sleeve / I
        let i = v_sleeve / r_14;

        // calculate R_RS using formula R_RS || R_3 = (V_Ring - V_Sleeve) / I
        let r_rs = 1.0 / (i / (v_ring - v_sleeve) - 1.0 / r_23);

        // calculate R_TR using formular R_TR || R_2 + R_1 = (V_CC - V_Ring) / I
        let r_tr = 1.0 / (1.0 / ((v_cc - v_ring) / i - 1.0 / r_14) - 1.0 / r_23);

        (r_tr, r_rs)
    }

    pub fn process(&mut self, adc: &mut StaticAdc<T>, midi_in: MidiMessageSender<'static>) {
        let v_ring = self.v_ring.read_voltage(adc);
        let v_sleeve = self.v_sleeve.read_voltage(adc);
        let (r_tip_ring, r_ring_sleve) = self.calculate_resistance(v_ring, v_sleeve);
    }

    pub fn update_settings(&mut self, settings: &ChannelSettings) {
        self.settings = *settings;
    }
}

pub struct ExpressionChannels(
    pub ExpressionChannel<ADC1>,
    pub ExpressionChannel<ADC1>,
    pub ExpressionChannel<ADC2>,
    pub ExpressionChannel<ADC2>,
);

pub struct ExpressionGroup {
    adc1: StaticAdc1,
    adc2: StaticAdc2,
    channels: ExpressionChannels,
}

impl ExpressionGroup {
    pub fn new(adc1: StaticAdc1, adc2: StaticAdc2, channels: ExpressionChannels) -> Self {
        Self {
            adc1,
            adc2,
            channels,
        }
    }

    pub fn process(&mut self, midi_in: MidiMessageSender<'static>) {
        // Process each channel individually
        self.channels.0.process(&mut self.adc1, midi_in);
        self.channels.1.process(&mut self.adc1, midi_in);
        self.channels.2.process(&mut self.adc2, midi_in);
        self.channels.3.process(&mut self.adc2, midi_in);
    }

    pub fn update_settings(&mut self, settings: &[ChannelSettings; EXPRESSION_NUM_CHANNELS]) {
        // Apply each channel's settings
        self.channels.0.update_settings(&settings[0]);
        self.channels.1.update_settings(&settings[1]);
        self.channels.2.update_settings(&settings[2]);
        self.channels.3.update_settings(&settings[3]);
    }
}

#[embassy_executor::task]
pub async fn task(mut exp: ExpressionGroup, midi_in: MidiMessageSender<'static>) {
    let interval = Duration::from_hz(EXPRESSION_POLL_HZ);

    loop {
        exp.process(midi_in);

        Timer::after(interval).await;
    }
}
