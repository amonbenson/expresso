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

    fn handle_midi(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::{MidiEndpoint, MidiMessage, MidiSink};
    use crate::settings::Settings;

    struct CaptureSink {
        pub message: Option<MidiMessage>,
        pub target: Option<MidiEndpoint>,
    }

    impl CaptureSink {
        fn new() -> Self {
            Self {
                message: None,
                target: None,
            }
        }
    }

    impl MidiSink for CaptureSink {
        fn emit(&mut self, message: MidiMessage, target: Option<MidiEndpoint>) {
            self.message = Some(message);
            self.target = target;
        }
    }

    const CC: MidiMessage = MidiMessage::ControlChange {
        channel: 0,
        control: 7,
        value: 64,
    };

    #[test]
    fn usb_source_routes_to_din() {
        let mut router = Router::new();
        let mut sink = CaptureSink::new();
        let mut settings = Settings::default();
        router
            .handle_midi(CC, MidiEndpoint::Usb, &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.target, Some(MidiEndpoint::Din));
        assert_eq!(sink.message, Some(CC));
    }

    #[test]
    fn din_source_routes_to_usb() {
        let mut router = Router::new();
        let mut sink = CaptureSink::new();
        let mut settings = Settings::default();
        router
            .handle_midi(CC, MidiEndpoint::Din, &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.target, Some(MidiEndpoint::Usb));
    }

    #[test]
    fn expression_source_routes_to_usb() {
        let mut router = Router::new();
        let mut sink = CaptureSink::new();
        let mut settings = Settings::default();
        router
            .handle_midi(CC, MidiEndpoint::Expression, &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.target, Some(MidiEndpoint::Usb));
    }

    #[test]
    fn message_is_forwarded_unchanged() {
        let msg = MidiMessage::NoteOn {
            channel: 3,
            note: 60,
            velocity: 100,
        };
        let mut router = Router::new();
        let mut sink = CaptureSink::new();
        let mut settings = Settings::default();
        router
            .handle_midi(msg, MidiEndpoint::Din, &mut sink, &mut settings)
            .unwrap();
        assert_eq!(sink.message, Some(msg));
    }
}
