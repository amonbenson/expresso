use libm::{expf, powf, roundf};
use snafu::Snafu;

use crate::midi::{MidiProcessor, MidiMessage, MidiSink};
use crate::settings::{ContinuousSettings, InputMode, Settings, SwitchSettings};

#[derive(Debug, Snafu)]
pub enum ExpressionChannelError {}

#[derive(Default)]
pub struct ExpressionChannel {
    index: usize,
    current_input: f32,
    previous_input: f32,
    current_output: u8,
    previous_output: u8,
}

impl ExpressionChannel {
    const V_CC: f32 = 3.3;
    const R_RAIL: f32 = 10.0;
    const R_PAR: f32 = 100.0;

    const R_MAX: f32 = 1_000_000_000.0;
    const R_THRESH: f32 = 10.0;

    const DRIVE_FACTOR: f32 = 5.0;

    pub fn from_index(index: usize) -> Self {
        Self {
            index,
            ..Default::default()
        }
    }

    pub fn index(&self) -> usize {
        self.index
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
}

impl<S> MidiProcessor<S> for ExpressionChannel
where
    S: MidiSink,
{
    type ProcessInputs = (f32, f32);
    type Error = ExpressionChannelError;

    fn process(
        &mut self,
        inputs: (f32, f32),
        sink: &mut S,
        settings: &mut Settings,
    ) -> Result<(), ExpressionChannelError>
    where
        S: MidiSink,
    {
        let settings = settings.expression.channels[self.index];

        // Calculate the resistance values
        let (v_ring, v_sleeve) = inputs;
        let (r_tip_ring, r_ring_sleeve) = Self::calculate_resistance(v_ring, v_sleeve);

        // Limit the resistance range
        let r_tip_ring = r_tip_ring.clamp(0.0, Self::R_MAX);
        let r_ring_sleeve = r_ring_sleeve.clamp(0.0, Self::R_MAX);
        let r_total = r_tip_ring + r_ring_sleeve;

        // Calculate the new input value
        self.previous_input = self.current_input;
        self.current_input = match settings.input.mode {
            InputMode::Continuous => r_ring_sleeve / r_total,
            InputMode::Switch => (r_total >= Self::R_THRESH) as u32 as f32,
            InputMode::Compat => v_ring / 3.3,
        }
        .clamp(0.0, 1.0);

        // Apply value transformations. This will also convert the input range 0..1 to MIDI range 0..127
        self.previous_output = self.current_output;
        self.current_output = match settings.input.mode {
            InputMode::Continuous => {
                Self::apply_continuous_transform(self.current_input, settings.input.continuous)
            }
            InputMode::Switch => {
                Self::apply_switch_transform(self.current_input, settings.input.switch)
            }
            InputMode::Compat => (((self.current_input - 0.4) / 0.22) * 127.0) as u8,
        };

        // Emit the new value if it changed. Use our index as the MIDI channel
        if self.current_output != self.previous_output {
            sink.emit(
                MidiMessage::ControlChange {
                    channel: (self.index as u8) % 128,
                    control: settings.cc,
                    value: self.current_output,
                },
                None,
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::midi::MidiEndpoint;
    use crate::settings::Settings;

    use super::*;

    // ---- Test helper ----

    #[derive(Debug)]
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

    impl MidiSink for MessageCollector {
        fn emit(&mut self, message: MidiMessage, _target: Option<MidiEndpoint>) {
            if let MidiMessage::ControlChange {
                channel,
                control,
                value,
            } = message
            {
                self.messages[self.count] = (channel, control, value);
                self.count += 1;
            }
        }
    }

    // ---- calculate_resistance ----

    #[test]
    fn resistance_symmetric() {
        // R_TR = R_RS = R_PAR = 100k (symmetric pedal at midpoint)
        // Gives v_sleeve=0.275, v_ring=1.65
        let (r1, r2) = ExpressionChannel::calculate_resistance(1.65, 0.275);
        assert!((r1 - 100.0).abs() < 0.01, "r1={r1}");
        assert!((r2 - 100.0).abs() < 0.01, "r2={r2}");
    }

    #[test]
    fn resistance_asymmetric() {
        // R_TR_circuit=200k, R_RS_circuit=50k
        // Derived: v_sleeve=0.275, v_ring=143/120≈1.1917
        // Function returns (r_tr_code=50, r_rs_code=200) due to internal naming
        let v_ring = 143.0_f32 / 120.0;
        let (r1, r2) = ExpressionChannel::calculate_resistance(v_ring, 0.275);
        assert!((r1 - 50.0).abs() < 0.01, "r1={r1}");
        assert!((r2 - 200.0).abs() < 0.01, "r2={r2}");
    }

    #[test]
    fn resistance_both_zero() {
        // v_ring = v_sleeve = v_tip = 1.65 (all nodes at same voltage, both pedal resistors shorted)
        let (r1, r2) = ExpressionChannel::calculate_resistance(1.65, 1.65);
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
            ExpressionChannel::apply_continuous_transform(0.0, linear_settings()),
            0
        );
    }

    #[test]
    fn continuous_full_maps_to_127() {
        assert_eq!(
            ExpressionChannel::apply_continuous_transform(1.0, linear_settings()),
            127
        );
    }

    #[test]
    fn continuous_midpoint_linear() {
        // 0.5 * 127 = 63.5, roundf -> 64
        assert_eq!(
            ExpressionChannel::apply_continuous_transform(0.5, linear_settings()),
            64
        );
    }

    #[test]
    fn continuous_quarter_linear() {
        // 0.25 * 127 = 31.75, roundf -> 32
        assert_eq!(
            ExpressionChannel::apply_continuous_transform(0.25, linear_settings()),
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
        assert_eq!(
            ExpressionChannel::apply_continuous_transform(0.5, settings),
            0
        ); // at min
        assert_eq!(
            ExpressionChannel::apply_continuous_transform(1.0, settings),
            127
        ); // at max
        assert_eq!(
            ExpressionChannel::apply_continuous_transform(0.75, settings),
            64
        ); // midpoint
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
        let out_linear = ExpressionChannel::apply_continuous_transform(0.5, low_drive);
        let out_driven = ExpressionChannel::apply_continuous_transform(0.5, high_drive);
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
                ExpressionChannel::apply_continuous_transform(0.0, s),
                0,
                "drive={drive}"
            );
            assert_eq!(
                ExpressionChannel::apply_continuous_transform(1.0, s),
                127,
                "drive={drive}"
            );
        }
    }

    // ---- apply_switch_transform ----

    #[test]
    fn switch_above_threshold_is_pressed() {
        let s = SwitchSettings::default(); // released=0, pressed=127
        assert_eq!(ExpressionChannel::apply_switch_transform(0.51, s), 127);
        assert_eq!(ExpressionChannel::apply_switch_transform(1.0, s), 127);
    }

    #[test]
    fn switch_below_threshold_is_released() {
        let s = SwitchSettings::default();
        assert_eq!(ExpressionChannel::apply_switch_transform(0.49, s), 0);
        assert_eq!(ExpressionChannel::apply_switch_transform(0.0, s), 0);
    }

    #[test]
    fn switch_at_threshold_boundary_is_released() {
        // The condition is strictly `value > 0.5`, so 0.5 itself is released
        let s = SwitchSettings::default();
        assert_eq!(ExpressionChannel::apply_switch_transform(0.5, s), 0);
    }

    #[test]
    fn switch_invert_polarity_flips_output() {
        let s = SwitchSettings {
            invert_polarity: true,
            ..SwitchSettings::default()
        };
        assert_eq!(ExpressionChannel::apply_switch_transform(0.6, s), 0); // would be pressed, inverted -> released
        assert_eq!(ExpressionChannel::apply_switch_transform(0.4, s), 127); // would be released, inverted -> pressed
    }

    #[test]
    fn switch_custom_pressed_released_values() {
        let s = SwitchSettings {
            invert_polarity: false,
            released_value: 20,
            pressed_value: 100,
        };
        assert_eq!(ExpressionChannel::apply_switch_transform(0.6, s), 100);
        assert_eq!(ExpressionChannel::apply_switch_transform(0.4, s), 20);
    }

    // ---- process ----

    #[test]
    fn process_first_call_sends_message() {
        // Initial output is 0; any real pedal position produces a non-zero value -> triggers send
        let mut settings = Settings::default();
        let mut sink = MessageCollector::new();
        let mut ch = ExpressionChannel::default();
        ch.process((1.65, 0.275), &mut sink, &mut settings).unwrap();
        assert_eq!(sink.count, 1);
    }

    #[test]
    fn process_no_message_when_output_unchanged() {
        let mut settings = Settings::default();
        let mut sink = MessageCollector::new();
        let mut ch = ExpressionChannel::default();
        ch.process((1.65, 0.275), &mut sink, &mut settings).unwrap();
        let count = sink.count;
        ch.process((1.65, 0.275), &mut sink, &mut settings).unwrap(); // same voltages -> same output
        assert_eq!(
            sink.count, count,
            "Expected no new message on unchanged output"
        );
    }

    #[test]
    fn process_sends_message_when_output_changes() {
        let mut settings = Settings::default();
        let mut sink = MessageCollector::new();
        let mut ch = ExpressionChannel::default();
        ch.process((1.65, 0.275), &mut sink, &mut settings).unwrap(); // input ≈ 0.5
        let count = sink.count;
        ch.process((143.0 / 120.0, 0.275), &mut sink, &mut settings)
            .unwrap(); // input ≈ 0.8 -> different output
        assert!(
            sink.count > count,
            "Expected a new message after output change"
        );
    }

    #[test]
    fn process_message_uses_correct_channel_and_cc() {
        // Channel index 3 -> MIDI channel 3, cc 5
        let mut settings = Settings::default();
        settings.expression.channels[3].cc = 5;
        let mut sink = MessageCollector::new();

        let mut ch = ExpressionChannel::from_index(3);

        ch.process((1.65, 0.275), &mut sink, &mut settings).unwrap();
        let (midi_ch, cc, _) = sink.messages[0];
        assert_eq!(midi_ch, 3);
        assert_eq!(cc, 5);
    }

    #[test]
    fn process_switch_active_on_high_resistance() {
        // r_total ≈ 200k >> R_THRESH=10k -> active -> pressed_value=127
        let mut settings = Settings::default();
        settings.expression.channels[0].input.mode = InputMode::Switch;
        let mut sink = MessageCollector::new();

        let mut ch = ExpressionChannel::default();

        ch.process((1.65, 0.275), &mut sink, &mut settings).unwrap();
        assert_eq!(sink.count, 1);
        assert_eq!(sink.last().2, 127);
    }

    #[test]
    fn process_switch_inactive_on_zero_resistance() {
        // First make it active, then short the pedal -> output goes to released_value=0
        let mut settings = Settings::default();
        settings.expression.channels[0].input.mode = InputMode::Switch;
        let mut sink = MessageCollector::new();

        let mut ch = ExpressionChannel::default();

        ch.process((1.65, 0.275), &mut sink, &mut settings).unwrap(); // active -> 127
        ch.process((1.65, 1.65), &mut sink, &mut settings).unwrap(); // r_total=0 < threshold -> released -> 0
        assert_eq!(sink.last().2, 0);
    }

    #[test]
    fn process_uses_updated_cc() {
        // Update CC to 42 before any processing
        let mut settings = Settings::default();
        settings.expression.channels[0].cc = 42;
        let mut sink = MessageCollector::new();
        let mut ch = ExpressionChannel::default();

        ch.process((1.65, 0.275), &mut sink, &mut settings).unwrap();
        assert_eq!(sink.count, 1);
        assert_eq!(
            sink.messages[0].1, 42,
            "Expected CC 42 after settings update"
        );
    }
}
