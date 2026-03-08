use snafu::Snafu;

use crate::midi::{MidiEndpoint, MidiHandler, MidiSink};

#[derive(Debug, Snafu)]
pub enum RouterError {}

pub struct Router {}

impl Router {
    pub fn new() -> Self {
        Self {}
    }
}

impl<S> MidiHandler<S> for Router
where
    S: MidiSink,
{
    type Error = RouterError;

    fn handle_message(
        &mut self,
        message: crate::midi::MidiMessage,
        source: crate::midi::MidiEndpoint,
        sink: &mut S,
        _settings: &mut crate::settings::Settings,
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
