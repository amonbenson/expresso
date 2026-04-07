use libm::{expf, powf, roundf};
use snafu::Snafu;

use crate::midi::{MidiGenerator, MidiMessage, MidiSink};
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

    const MIN_INPUT_DELTA: f32 = 0.02;

    pub fn from_index(index: usize) -> Self {
        Self {
            index,
            ..Default::default()
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn calculate_resistance(v_ring: f32, v_tip: f32) -> (f32, f32) {
        // calculate I
        let i = (Self::V_CC - v_tip) / Self::R_RAIL;

        // calculate V_tip
        let v_sleeve = Self::V_CC - v_tip;

        // calculate R_TR
        let r_tr = (Self::R_PAR * (v_tip - v_ring)) / (i * Self::R_PAR + v_ring - v_tip);

        // calculate R_RS
        let r_rs = (Self::R_PAR * (v_ring - v_sleeve)) / (i * Self::R_PAR + v_sleeve - v_ring);

        (r_tr, r_rs)
    }

    pub fn apply_continuous_transform(value: f32, settings: ContinuousSettings) -> u8 {
        // Apply input scaling
        let value =
            (value - settings.minimum_input) / (settings.maximum_input - settings.minimum_input);

        // Apply drive
        let exponent = expf(-(settings.drive * 2.0 - 1.0) * Self::DRIVE_FACTOR);
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

impl<S> MidiGenerator<S> for ExpressionChannel
where
    S: MidiSink,
{
    type Inputs = (f32, f32);
    type Error = ExpressionChannelError;

    fn generate_midi(
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
        let (v_ring, v_tip) = inputs;
        let (r_tip_ring, r_ring_sleeve) = Self::calculate_resistance(v_ring, v_tip);

        // Limit the resistance range
        let r_tip_ring = r_tip_ring.clamp(0.0, Self::R_MAX);
        let r_ring_sleeve = r_ring_sleeve.clamp(0.0, Self::R_MAX);
        let r_total = r_tip_ring + r_ring_sleeve;

        // Calculate the new input value
        let new_input = match settings.input.mode {
            InputMode::Continuous => r_ring_sleeve / r_total,
            InputMode::Switch => (r_total >= Self::R_THRESH) as u32 as f32,
            InputMode::Compat => v_ring / 3.3,
        }
        .clamp(0.0, 1.0);

        // Ignore changes smaller than the minimum delta to suppress noise
        if (new_input - self.current_input).abs() < Self::MIN_INPUT_DELTA {
            return Ok(());
        }

        self.previous_input = self.current_input;
        self.current_input = new_input;

        // Apply value transformations. This will also convert the input range 0..1 to MIDI range 0..127
        self.previous_output = self.current_output;
        self.current_output = match settings.input.mode {
            InputMode::Continuous => {
                Self::apply_continuous_transform(self.current_input, settings.input.continuous)
            }
            InputMode::Switch => {
                Self::apply_switch_transform(self.current_input, settings.input.switch)
            }
            InputMode::Compat => {
                (((self.current_input - 0.4) / 0.22).clamp(0.0, 1.0) * 127.0) as u8
            }
        };

        // Emit the new value if it changed. Use our index as the MIDI channel
        if self.current_output == self.previous_output {
            return Ok(());
        }

        sink.emit(
            MidiMessage::ControlChange {
                channel: (self.index as u8) % 128,
                control: settings.cc,
                value: self.current_output,
            },
            None,
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::expression::test_utils::MessageCollector;
    use crate::settings::Settings;

    use super::*;

    // ---- calculate_resistance ----

    #[test]
    fn resistance_symmetric() {
        // R_TR = R_RS = 100k. Each leg has R_PAR=100k in parallel -> 50k effective per leg.
        // Total load = 100k, i = 3.3/110k = 0.03mA, v_tip = 3.0V, v_ring = 0.03*50 = 1.5V.
        let (r_tr, r_rs) = ExpressionChannel::calculate_resistance(1.5, 3.0);
        assert!((r_tr - 100.0).abs() < 0.01, "r_tr={r_tr}");
        assert!((r_rs - 100.0).abs() < 0.01, "r_rs={r_rs}");
    }

    #[test]
    fn resistance_asymmetric() {
        // R_TR=200k, R_RS=50k. Parallel legs: 200||100=66.67k, 50||100=33.33k.
        // Total load = 100k, i = 0.03mA, v_tip = 3.0V, v_ring = 0.03*33.33 = 1.0V.
        let (r_tr, r_rs) = ExpressionChannel::calculate_resistance(1.0, 3.0);
        assert!((r_tr - 200.0).abs() < 0.01, "r_tr={r_tr}");
        assert!((r_rs - 50.0).abs() < 0.01, "r_rs={r_rs}");
    }

    #[test]
    fn resistance_minimum_pedal() {
        // Pedal at minimum (wiper at sleeve/GND end): R_RS = 0, v_ring = 0.
        // With R_TR=100k: R_TR||R_PAR = 50k, i = 3.3/60k = 0.055mA, v_tip = 2.75V.
        let (r_tr, r_rs) = ExpressionChannel::calculate_resistance(0.0, 2.75);
        assert!((r_tr - 100.0).abs() < 0.01, "r_tr={r_tr}");
        assert!((r_rs - 0.0).abs() < 0.01, "r_rs={r_rs}");
    }

    #[test]
    fn resistance_maximum_pedal() {
        // Pedal at maximum (wiper at tip end): R_TR = 0, so v_ring = v_tip.
        // With R_RS=100k: both legs 50k, i = 0.055mA, v_tip = v_ring = 2.75V.
        let (r_tr, r_rs) = ExpressionChannel::calculate_resistance(2.75, 2.75);
        assert!((r_tr - 0.0).abs() < 0.01, "r_tr={r_tr}");
        assert!((r_rs - 100.0).abs() < 0.01, "r_rs={r_rs}");
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
        // Symmetric pedal midpoint: v_ring=1.5V, v_tip=3.0V -> position=0.5, CC≠0 -> triggers send
        let mut settings = Settings::default();
        let mut sink = MessageCollector::new();
        let mut ch = ExpressionChannel::default();
        ch.generate_midi((1.5, 3.0), &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.count, 1);
    }

    #[test]
    fn process_no_message_when_output_unchanged() {
        let mut settings = Settings::default();
        let mut sink = MessageCollector::new();
        let mut ch = ExpressionChannel::default();
        ch.generate_midi((1.5, 3.0), &mut sink, &mut settings)
            .unwrap();
        let count = sink.count;
        ch.generate_midi((1.5, 3.0), &mut sink, &mut settings)
            .unwrap(); // same voltages -> same output
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
        // Symmetric midpoint: R_TR=R_RS=100k, position=0.5
        ch.generate_midi((1.5, 3.0), &mut sink, &mut settings)
            .unwrap();
        let count = sink.count;
        // Asymmetric: R_TR=200k, R_RS=50k, position=0.2 -> clearly different output
        ch.generate_midi((1.0, 3.0), &mut sink, &mut settings)
            .unwrap();
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

        ch.generate_midi((1.5, 3.0), &mut sink, &mut settings)
            .unwrap();
        let (midi_ch, cc, _) = sink.messages[0];
        assert_eq!(midi_ch, 3);
        assert_eq!(cc, 5);
    }

    #[test]
    fn process_switch_active_on_high_resistance() {
        // Symmetric midpoint: r_total = 200k >> R_THRESH=10k -> active -> pressed_value=127
        let mut settings = Settings::default();
        settings.expression.channels[0].input.mode = InputMode::Switch;
        let mut sink = MessageCollector::new();

        let mut ch = ExpressionChannel::default();

        ch.generate_midi((1.5, 3.0), &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.count, 1);
        assert_eq!(sink.last().2, 127);
    }

    #[test]
    fn process_switch_inactive_on_zero_resistance() {
        // First make it active, then short tip+ring to GND -> r_total=0 < threshold -> released -> 0
        let mut settings = Settings::default();
        settings.expression.channels[0].input.mode = InputMode::Switch;
        let mut sink = MessageCollector::new();

        let mut ch = ExpressionChannel::default();

        ch.generate_midi((1.5, 3.0), &mut sink, &mut settings)
            .unwrap(); // active -> 127
        ch.generate_midi((0.0, 0.0), &mut sink, &mut settings)
            .unwrap(); // r_total=0 < threshold -> released -> 0
        assert_eq!(sink.last().2, 0);
    }

    #[test]
    fn process_uses_updated_cc() {
        // Update CC to 42 before any processing
        let mut settings = Settings::default();
        settings.expression.channels[0].cc = 42;
        let mut sink = MessageCollector::new();
        let mut ch = ExpressionChannel::default();

        ch.generate_midi((1.5, 3.0), &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.count, 1);
        assert_eq!(
            sink.messages[0].1, 42,
            "Expected CC 42 after settings update"
        );
    }
}
