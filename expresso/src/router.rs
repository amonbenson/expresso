use snafu::Snafu;

use crate::component::Component;
use crate::midi::MidiMessageSink;
use crate::midi::types::MidiEndpoint;

#[derive(Debug, Snafu)]
pub enum RouterError {}

pub struct Router {}

impl Router {
    pub fn new() -> Self {
        Self {}
    }
}

impl<const C: usize, S: MidiMessageSink> Component<C, S> for Router {
    type ProcessInputs = ();
    type Error = RouterError;

    fn handle_message(
        &mut self,
        message: crate::midi::MidiMessage,
        source: crate::midi::types::MidiEndpoint,
        sink: &mut S,
        _settings: &mut crate::settings::Settings<C>,
    ) -> Result<(), RouterError> {
        // Use Usb and Din in bridge configuration and route Expression messages to the USB interface
        let target = match source {
            MidiEndpoint::Usb => MidiEndpoint::Din,
            MidiEndpoint::Din => MidiEndpoint::Usb,
            MidiEndpoint::Expression => MidiEndpoint::Usb,
        };
        sink.emit(message, target.into());

        Ok(())
    }
}
