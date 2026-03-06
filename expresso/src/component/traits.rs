use crate::midi::types::MidiEndpoint;
use crate::midi::{MidiMessage, MidiMessageSink};
use crate::settings::Settings;

pub trait Component<const C: usize, S: MidiMessageSink> {
    type ProcessInputs;
    type Error: snafu::Error;

    fn handle_message(
        &mut self,
        message: MidiMessage,
        source: MidiEndpoint,
        sink: &mut S,
        settings: &mut Settings<C>,
    ) -> Result<(), Self::Error> {
        let _ = message;
        let _ = source;
        let _ = sink;
        let _ = settings;
        Ok(())
    }

    fn process(
        &mut self,
        inputs: Self::ProcessInputs,
        sink: &mut S,
        settings: &mut Settings<C>,
    ) -> Result<(), Self::Error> {
        let _ = inputs;
        let _ = sink;
        let _ = settings;
        Ok(())
    }
}
