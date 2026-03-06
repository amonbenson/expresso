use core::iter::zip;

use snafu::{ResultExt, Snafu};

use super::channel::{ExpressionChannel, ExpressionChannelError};
use crate::component::Component;
use crate::midi::MidiMessageSink;

#[derive(Debug, Snafu)]
pub enum ExpressionGroupError {
    #[snafu(display("channel {index} failed"))]
    Channel {
        index: usize,
        source: ExpressionChannelError,
    },
}

pub struct ExpressionGroup<const C: usize> {
    channels: [ExpressionChannel; C],
}

impl<const C: usize> ExpressionGroup<C> {
    pub fn new() -> Self {
        Self {
            channels: core::array::from_fn(ExpressionChannel::from_index),
        }
    }
}

impl<const C: usize, S: MidiMessageSink> Component<C, S> for ExpressionGroup<C> {
    type ProcessInputs = [(f32, f32); C];
    type Error = ExpressionGroupError;

    fn process(
        &mut self,
        inputs: Self::ProcessInputs,
        sink: &mut S,
        settings: &mut crate::settings::Settings<C>,
    ) -> Result<(), ExpressionGroupError> {
        // Process each channel
        for (channel, input) in zip(self.channels.iter_mut(), inputs) {
            let index = channel.index();
            channel
                .process(input, sink, settings)
                .context(ChannelSnafu { index })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::midi::{MidiMessage, MidiMessageSink};
    use crate::settings::Settings;

    struct MessageCollector {
        count: usize,
    }

    impl MessageCollector {
        fn new() -> Self {
            Self { count: 0 }
        }
    }

    impl MidiMessageSink for MessageCollector {
        fn emit(&mut self, _message: MidiMessage) {
            self.count += 1;
        }
    }

    // All channels process independently: N channels -> up to N messages per call.
    #[test]
    fn all_channels_emit_on_first_call() {
        let mut group = ExpressionGroup::<4>::new();
        let mut sink = MessageCollector::new();
        let mut settings = Settings::<4>::default();
        // Symmetric midpoint input produces a non-zero output on each channel.
        group
            .process([(1.65, 0.275); 4], &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.count, 4);
    }

    // Single-channel group works (const generic edge case).
    #[test]
    fn single_channel_group() {
        let mut group = ExpressionGroup::<1>::new();
        let mut sink = MessageCollector::new();
        let mut settings = Settings::<1>::default();
        group
            .process([(1.65, 0.275)], &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.count, 1);
    }

    // No messages emitted when all channels repeat the same output.
    #[test]
    fn no_messages_when_all_outputs_unchanged() {
        let mut group = ExpressionGroup::<2>::new();
        let mut sink = MessageCollector::new();
        let mut settings = Settings::<2>::default();
        group
            .process([(1.65, 0.275); 2], &mut sink, &mut settings)
            .unwrap();
        let after_first = sink.count;
        group
            .process([(1.65, 0.275); 2], &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.count, after_first);
    }

    // Only the changed channel emits a new message.
    #[test]
    fn only_changed_channel_emits() {
        let mut group = ExpressionGroup::<2>::new();
        let mut sink = MessageCollector::new();
        let mut settings = Settings::<2>::default();
        group
            .process([(1.65, 0.275); 2], &mut sink, &mut settings)
            .unwrap();
        let after_first = sink.count;
        // Move channel 0 to a very different position; keep channel 1 unchanged.
        group
            .process(
                [(143.0 / 120.0, 0.275), (1.65, 0.275)],
                &mut sink,
                &mut settings,
            )
            .unwrap();
        assert_eq!(sink.count, after_first + 1);
    }
}
