use super::types::{DecodeResult, MidiEndpoint, MidiMessage};

pub trait MidiMessageSink {
    fn emit(&mut self, message: MidiMessage, target: Option<MidiEndpoint>);
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
