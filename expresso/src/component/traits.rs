use crate::midi::{MidiMessage, MidiMessageSink};
use crate::settings::Settings;

pub trait Component<const C: usize, S: MidiMessageSink> {
    type ProcessInputs;
    type Error: snafu::Error;

    fn handle_message(
        &mut self,
        msg: MidiMessage,
        sink: &mut S,
        settings: &mut Settings<C>,
    ) -> Result<(), Self::Error> {
        let _ = msg;
        let _ = sink;
        let _ = settings;
        Ok(())
    }

    fn process(
        &mut self,
        inputs: Self::ProcessInputs,
        sink: &mut S,
        settings: &mut Settings<C>,
    ) -> Result<(), Self::Error>;
}
