use crate::settings::Settings;

use super::types::{DecodeResult, MidiEndpoint, MidiMessage};

pub trait MidiSink {
    fn emit(&mut self, message: MidiMessage, target: Option<MidiEndpoint>);
}

pub trait MidiHandler<S>
where
    S: MidiSink,
{
    type Error: snafu::Error;

    fn handle_message(
        &mut self,
        message: MidiMessage,
        source: MidiEndpoint,
        sink: &mut S,
        settings: &mut Settings,
    ) -> Result<(), Self::Error>;
}

pub trait MidiProcessor<S>
where
    S: MidiSink,
{
    type ProcessInputs;
    type Error: snafu::Error;

    fn process(
        &mut self,
        inputs: Self::ProcessInputs,
        sink: &mut S,
        settings: &mut Settings,
    ) -> Result<(), Self::Error>;
}

pub trait PacketSink {
    type Packet;
    type Error;

    fn emit(&mut self, packet: Self::Packet) -> Result<(), Self::Error>;
}

pub trait MidiEncoder {
    type Packet;

    fn emit<S>(&mut self, message: &MidiMessage, sink: &mut S) -> Result<(), S::Error>
    where
        S: PacketSink<Packet = Self::Packet>;
}

pub trait MidiDecoder {
    type Packet;

    fn feed(&mut self, packet: Self::Packet) -> Option<DecodeResult<'_>>;
    fn reset(&mut self);
}
