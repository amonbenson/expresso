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
            channel
                .process(input, sink, settings)
                .context(ChannelSnafu {
                    index: channel.index(),
                })?;
        }

        Ok(())
    }
}
