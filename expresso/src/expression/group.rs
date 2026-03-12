use core::iter::zip;

use snafu::{ResultExt, Snafu};

use super::channel::{ExpressionChannel, ExpressionChannelError};
use crate::config::NUM_CHANNELS;
use crate::midi::{MidiGenerator, MidiSink};

#[derive(Debug, Snafu)]
pub enum ExpressionGroupError {
    #[snafu(display("channel {index} failed"))]
    Channel {
        index: usize,
        source: ExpressionChannelError,
    },
}

pub struct ExpressionGroup {
    channels: [ExpressionChannel; NUM_CHANNELS],
}

impl ExpressionGroup {
    pub fn new() -> Self {
        Self {
            channels: core::array::from_fn(ExpressionChannel::from_index),
        }
    }
}

impl<S> MidiGenerator<S> for ExpressionGroup
where
    S: MidiSink,
{
    type Inputs = [(f32, f32); NUM_CHANNELS];
    type Error = ExpressionGroupError;

    fn generate_midi(
        &mut self,
        inputs: Self::Inputs,
        sink: &mut S,
        settings: &mut crate::settings::Settings,
    ) -> Result<(), ExpressionGroupError> {
        // Process each channel
        for (channel, input) in zip(self.channels.iter_mut(), inputs) {
            let index = channel.index();
            channel
                .generate_midi(input, sink, settings)
                .context(ChannelSnafu { index })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::test_utils::MessageCollector;
    use crate::settings::Settings;

    // All channels process independently: N channels -> up to N messages per call.
    #[test]
    fn all_channels_emit_on_first_call() {
        let mut group = ExpressionGroup::new();
        let mut sink = MessageCollector::new();
        let mut settings = Settings::default();
        group
            .generate_midi([(1.65, 0.275); 4], &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.count, 4);
    }

    // Each channel uses its index as the MIDI channel number.
    #[test]
    fn each_channel_uses_correct_midi_channel() {
        let mut group = ExpressionGroup::new();
        let mut sink = MessageCollector::new();
        let mut settings = Settings::default();
        group
            .generate_midi([(1.65, 0.275); 4], &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.count, 4);
        for i in 0..4 {
            assert_eq!(
                sink.messages[i].0, i as u8,
                "channel {i} wrong MIDI channel"
            );
        }
    }

    // Each channel uses the CC number from settings.
    #[test]
    fn each_channel_uses_correct_cc() {
        let mut group = ExpressionGroup::new();
        let mut sink = MessageCollector::new();
        let mut settings = Settings::default();
        settings.expression.channels[0].cc = 10;
        settings.expression.channels[1].cc = 20;
        settings.expression.channels[2].cc = 30;
        settings.expression.channels[3].cc = 40;
        group
            .generate_midi([(1.65, 0.275); 4], &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.count, 4);
        // Messages are emitted in channel order (0, 1, 2).
        assert_eq!(sink.messages[0].1, 10, "channel 0 wrong CC");
        assert_eq!(sink.messages[1].1, 20, "channel 1 wrong CC");
        assert_eq!(sink.messages[2].1, 30, "channel 2 wrong CC");
        assert_eq!(sink.messages[3].1, 40, "channel 3 wrong CC");
    }

    // No messages emitted when all channels repeat the same output.
    #[test]
    fn no_messages_when_all_outputs_unchanged() {
        let mut group = ExpressionGroup::new();
        let mut sink = MessageCollector::new();
        let mut settings = Settings::default();
        group
            .generate_midi([(1.65, 0.275); 4], &mut sink, &mut settings)
            .unwrap();
        let after_first = sink.count;
        group
            .generate_midi([(1.65, 0.275); 4], &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.count, after_first);
    }

    // Only the changed channel emits a new message, with the correct channel number.
    #[test]
    fn only_changed_channel_emits() {
        let mut group = ExpressionGroup::new();
        let mut sink = MessageCollector::new();
        let mut settings = Settings::default();
        group
            .generate_midi([(1.65, 0.275); 4], &mut sink, &mut settings)
            .unwrap();
        let after_first = sink.count;
        // Move channel 1 to a very different position; keep other channels unchanged.
        group
            .generate_midi(
                [
                    (1.65, 0.275),
                    (143.0 / 120.0, 0.275),
                    (1.65, 0.275),
                    (1.65, 0.275),
                ],
                &mut sink,
                &mut settings,
            )
            .unwrap();
        assert_eq!(sink.count, after_first + 1);
        // The new message must come from MIDI channel 1.
        assert_eq!(sink.last().0, 1, "expected message from MIDI channel 1");
    }
}
