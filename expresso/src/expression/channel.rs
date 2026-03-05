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

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Test helper ----

    struct MessageCollector {
        messages: [(u8, u8, u8); 16], // (channel, control, value)
        count: usize,
    }

    impl MessageCollector {
        fn new() -> Self {
            Self {
                messages: [(0, 0, 0); 16],
                count: 0,
            }
        }

        fn last(&self) -> (u8, u8, u8) {
            self.messages[self.count - 1]
        }
    }

    #[derive(Debug)]
    struct SinkError;

    impl MidiMessageSink for MessageCollector {
        type Error = SinkError;

        fn try_send(&mut self, message: MidiMessage) -> Result<(), SinkError> {
            if let MidiMessage::ControlChange {
                channel,
                control,
                value,
            } = message
            {
                self.messages[self.count] = (channel, control, value);
                self.count += 1;
            }
            Ok(())
        }
    }

    // ---- calculate_resistance ----

    #[test]
    fn resistance_symmetric() {
        // R_TR = R_RS = R_PAR = 100k (symmetric pedal at midpoint)
        // Gives v_sleeve=0.275, v_ring=1.65
        let (r1, r2) = Channel::calculate_resistance(1.65, 0.275);
        assert!((r1 - 100.0).abs() < 0.01, "r1={r1}");
        assert!((r2 - 100.0).abs() < 0.01, "r2={r2}");
    }

    #[test]
    fn resistance_asymmetric() {
        // R_TR_circuit=200k, R_RS_circuit=50k
        // Derived: v_sleeve=0.275, v_ring=143/120≈1.1917
        // Function returns (r_tr_code=50, r_rs_code=200) due to internal naming
        let v_ring = 143.0_f32 / 120.0;
        let (r1, r2) = Channel::calculate_resistance(v_ring, 0.275);
        assert!((r1 - 50.0).abs() < 0.01, "r1={r1}");
        assert!((r2 - 200.0).abs() < 0.01, "r2={r2}");
    }

    #[test]
    fn resistance_both_zero() {
        // v_ring = v_sleeve = v_tip = 1.65 (all nodes at same voltage, both pedal resistors shorted)
        let (r1, r2) = Channel::calculate_resistance(1.65, 1.65);
        assert!((r1 - 0.0).abs() < 0.01, "r1={r1}");
        assert!((r2 - 0.0).abs() < 0.01, "r2={r2}");
    }

    // ---- apply_continuous_transform ----

    fn linear_settings() -> ContinuousSettings {
        ContinuousSettings {
            drive: 0.0,
            ..ContinuousSettings::default()
        }
    }

    #[test]
    fn continuous_zero_maps_to_zero() {
        assert_eq!(
            Channel::apply_continuous_transform(0.0, linear_settings()),
            0
        );
    }

    #[test]
    fn continuous_full_maps_to_127() {
        assert_eq!(
            Channel::apply_continuous_transform(1.0, linear_settings()),
            127
        );
    }

    #[test]
    fn continuous_midpoint_linear() {
        // 0.5 * 127 = 63.5, roundf → 64
        assert_eq!(
            Channel::apply_continuous_transform(0.5, linear_settings()),
            64
        );
    }

    #[test]
    fn continuous_quarter_linear() {
        // 0.25 * 127 = 31.75, roundf → 32
        assert_eq!(
            Channel::apply_continuous_transform(0.25, linear_settings()),
            32
        );
    }

    #[test]
    fn continuous_input_scaling() {
        // Only 0.5–1.0 of the physical range maps to the full MIDI range
        let settings = ContinuousSettings {
            minimum_input: 0.5,
            maximum_input: 1.0,
            drive: 0.0,
            ..ContinuousSettings::default()
        };
        assert_eq!(Channel::apply_continuous_transform(0.5, settings), 0); // at min
        assert_eq!(Channel::apply_continuous_transform(1.0, settings), 127); // at max
        assert_eq!(Channel::apply_continuous_transform(0.75, settings), 64); // midpoint
    }

    #[test]
    fn continuous_drive_pushes_midpoint_higher() {
        // Higher drive = smaller exponent from exp(-drive*5) = curve bowed toward max
        let low_drive = ContinuousSettings {
            drive: 0.0,
            ..ContinuousSettings::default()
        };
        let high_drive = ContinuousSettings {
            drive: 1.0,
            ..ContinuousSettings::default()
        };
        let out_linear = Channel::apply_continuous_transform(0.5, low_drive);
        let out_driven = Channel::apply_continuous_transform(0.5, high_drive);
        assert!(
            out_driven > out_linear,
            "out_driven={out_driven} out_linear={out_linear}"
        );
    }

    #[test]
    fn continuous_drive_does_not_affect_endpoints() {
        for drive in [0.0_f32, 0.5, 1.0] {
            let s = ContinuousSettings {
                drive,
                ..ContinuousSettings::default()
            };
            assert_eq!(
                Channel::apply_continuous_transform(0.0, s),
                0,
                "drive={drive}"
            );
            assert_eq!(
                Channel::apply_continuous_transform(1.0, s),
                127,
                "drive={drive}"
            );
        }
    }

    // ---- apply_switch_transform ----

    #[test]
    fn switch_above_threshold_is_pressed() {
        let s = SwitchSettings::default(); // released=0, pressed=127
        assert_eq!(Channel::apply_switch_transform(0.51, s), 127);
        assert_eq!(Channel::apply_switch_transform(1.0, s), 127);
    }

    #[test]
    fn switch_below_threshold_is_released() {
        let s = SwitchSettings::default();
        assert_eq!(Channel::apply_switch_transform(0.49, s), 0);
        assert_eq!(Channel::apply_switch_transform(0.0, s), 0);
    }

    #[test]
    fn switch_at_threshold_boundary_is_released() {
        // The condition is strictly `value > 0.5`, so 0.5 itself is released
        let s = SwitchSettings::default();
        assert_eq!(Channel::apply_switch_transform(0.5, s), 0);
    }

    #[test]
    fn switch_invert_polarity_flips_output() {
        let s = SwitchSettings {
            invert_polarity: true,
            ..SwitchSettings::default()
        };
        assert_eq!(Channel::apply_switch_transform(0.6, s), 0); // would be pressed, inverted → released
        assert_eq!(Channel::apply_switch_transform(0.4, s), 127); // would be released, inverted → pressed
    }

    #[test]
    fn switch_custom_pressed_released_values() {
        let s = SwitchSettings {
            invert_polarity: false,
            released_value: 20,
            pressed_value: 100,
        };
        assert_eq!(Channel::apply_switch_transform(0.6, s), 100);
        assert_eq!(Channel::apply_switch_transform(0.4, s), 20);
    }

    // ---- process ----

    #[test]
    fn process_first_call_sends_message() {
        // Initial output is 0; any real pedal position produces a non-zero value → triggers send
        let mut ch = Channel::new(0);
        let mut sink = MessageCollector::new();
        ch.process(1.65, 0.275, &mut sink).unwrap();
        assert_eq!(sink.count, 1);
    }

    #[test]
    fn process_no_message_when_output_unchanged() {
        let mut ch = Channel::new(0);
        let mut sink = MessageCollector::new();
        ch.process(1.65, 0.275, &mut sink).unwrap();
        let count = sink.count;
        ch.process(1.65, 0.275, &mut sink).unwrap(); // same voltages → same output
        assert_eq!(
            sink.count, count,
            "Expected no new message on unchanged output"
        );
    }

    #[test]
    fn process_sends_message_when_output_changes() {
        let mut ch = Channel::new(0);
        let mut sink = MessageCollector::new();
        ch.process(1.65, 0.275, &mut sink).unwrap(); // input ≈ 0.5
        let count = sink.count;
        ch.process(143.0 / 120.0, 0.275, &mut sink).unwrap(); // input ≈ 0.8 → different output
        assert!(
            sink.count > count,
            "Expected a new message after output change"
        );
    }

    #[test]
    fn process_message_uses_correct_channel_and_cc() {
        // Channel index 5 → MIDI channel 5, cc 5
        let mut ch = Channel::new(5);
        let mut sink = MessageCollector::new();
        ch.process(1.65, 0.275, &mut sink).unwrap();
        let (midi_ch, cc, _) = sink.messages[0];
        assert_eq!(midi_ch, 5);
        assert_eq!(cc, 5);
    }

    #[test]
    fn process_switch_active_on_high_resistance() {
        // r_total ≈ 200k >> R_THRESH=10k → active → pressed_value=127
        let mut ch = Channel::new(0);
        let mut settings = ChannelSettings::new(0);
        settings.input.mode = InputMode::Switch;
        ch.update_settings(&settings);

        let mut sink = MessageCollector::new();
        ch.process(1.65, 0.275, &mut sink).unwrap();
        assert_eq!(sink.count, 1);
        assert_eq!(sink.last().2, 127);
    }

    #[test]
    fn process_switch_inactive_on_zero_resistance() {
        // First make it active, then short the pedal → output goes to released_value=0
        let mut ch = Channel::new(0);
        let mut settings = ChannelSettings::new(0);
        settings.input.mode = InputMode::Switch;
        ch.update_settings(&settings);

        let mut sink = MessageCollector::new();
        ch.process(1.65, 0.275, &mut sink).unwrap(); // active → 127
        ch.process(1.65, 1.65, &mut sink).unwrap(); // r_total=0 < threshold → released → 0
        assert_eq!(sink.last().2, 0);
    }

    #[test]
    fn process_uses_updated_cc() {
        let mut ch = Channel::new(0);

        // Update CC to 42 before any processing
        let mut settings = ChannelSettings::new(0);
        settings.cc = 42;
        ch.update_settings(&settings);

        let mut sink = MessageCollector::new();
        ch.process(1.65, 0.275, &mut sink).unwrap();
        assert_eq!(sink.count, 1);
        assert_eq!(
            sink.messages[0].1, 42,
            "Expected CC 42 after settings update"
        );
    }
}
