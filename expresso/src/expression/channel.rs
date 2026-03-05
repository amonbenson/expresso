use libm::{expf, powf, roundf};

use crate::midi::{MidiMessage, MidiMessageSink};
use crate::settings::{Adjustable, ChannelSettings, ContinuousSettings, InputMode, SwitchSettings};

pub struct Channel {
    settings: ChannelSettings,
    index: usize,
    current_input: f32,
    previous_input: f32,
    current_output: u8,
    previous_output: u8,
}

impl Channel {
    const V_CC: f32 = 3.3;
    const R_RAIL: f32 = 10.0;
    const R_PAR: f32 = 100.0;

    const R_MAX: f32 = 1_000_000_000.0;
    const R_THRESH: f32 = 10.0;

    const DRIVE_FACTOR: f32 = 5.0;

    pub fn new(index: usize) -> Self {
        Self {
            settings: ChannelSettings {
                cc: (index as u8) % 128,
                ..Default::default()
            },
            index,
            current_input: 0.0,
            previous_input: 0.0,
            current_output: 0,
            previous_output: 0,
        }
    }

    pub fn calculate_resistance(v_ring: f32, v_sleeve: f32) -> (f32, f32) {
        // calculate I
        let i = v_sleeve / Self::R_RAIL;

        // calculate V_tip
        let v_tip = Self::V_CC - Self::R_RAIL * i;

        // calculate R_RS
        let r_rs = (Self::R_PAR * (v_tip - v_ring)) / (i * Self::R_PAR + v_ring - v_tip);

        // calculate R_TR
        let r_tr = (Self::R_PAR * (v_ring - v_sleeve)) / (i * Self::R_PAR + v_sleeve - v_ring);

        (r_tr, r_rs)
    }

    pub fn apply_continuous_transform(value: f32, settings: ContinuousSettings) -> u8 {
        // Apply input scaling
        let value =
            (value - settings.minimum_input) / (settings.maximum_input - settings.minimum_input);

        // Apply drive
        let exponent = expf(-settings.drive * Self::DRIVE_FACTOR);
        let value = powf(value, exponent);

        // Apply output transform
        let value = roundf(
            (value + settings.minimum_output as f32)
                * (settings.maximum_output - settings.minimum_output) as f32,
        ) as u8;

        value
    }

    pub fn apply_switch_transform(value: f32, settings: SwitchSettings) -> u8 {
        let active = value > 0.5;

        // Apply inversion
        let active = active != settings.invert_polarity;

        // Apply output transform
        let value = if active {
            settings.pressed_value
        } else {
            settings.released_value
        };

        value
    }

    pub fn process<S>(&mut self, v_ring: f32, v_sleeve: f32, sink: &mut S) -> Result<(), S::Error>
    where
        S: MidiMessageSink,
    {
        // Calculate the resistance values
        let (r_tip_ring, r_ring_sleeve) = Self::calculate_resistance(v_ring, v_sleeve);

        // Limit the resistance range
        let r_tip_ring = r_tip_ring.clamp(0.0, Self::R_MAX);
        let r_ring_sleeve = r_ring_sleeve.clamp(0.0, Self::R_MAX);
        let r_total = r_tip_ring + r_ring_sleeve;

        // Calculate the new input value
        self.previous_input = self.current_input;
        self.current_input = match self.settings.input.mode {
            InputMode::Continuous => r_ring_sleeve / r_total,
            InputMode::Switch => (r_total >= Self::R_THRESH) as u32 as f32,
        }
        .clamp(0.0, 1.0);

        // Apply value transformations. This will also convert the input range 0..1 to MIDI range 0..127
        self.previous_output = self.current_output;
        self.current_output = match self.settings.input.mode {
            InputMode::Continuous => {
                Self::apply_continuous_transform(self.current_input, self.settings.input.continuous)
            }
            InputMode::Switch => {
                Self::apply_switch_transform(self.current_input, self.settings.input.switch)
            }
        };

        // Update the value if it changed
        if self.current_output != self.previous_output {
            sink.try_send(MidiMessage::ControlChange {
                channel: (self.index as u8) % 128, // Use index as channel internally
                control: self.settings.cc,
                value: self.current_output,
            })?;
        }

        Ok(())
    }
}

impl Adjustable for Channel {
    type Settings = ChannelSettings;

    fn update_settings(&mut self, settings: &ChannelSettings) {
        // Settings will automatically take effect on the next process() call
        self.settings = *settings;
    }
}
