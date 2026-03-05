use super::types::MidiMessage;

pub trait MidiMessageSink {
    type Error;

    fn try_send(&mut self, message: MidiMessage) -> Result<(), Self::Error>;
}

pub trait PacketSink {
    type Packet;
    type Error;

    fn try_send(&mut self, packet: Self::Packet) -> Result<(), Self::Error>;
}

pub trait MidiEncoder {
    type Packet;

    fn emit<S>(&mut self, message: &MidiMessage<'_>, sink: &mut S) -> Result<(), S::Error>
    where
        S: PacketSink<Packet = Self::Packet>;
}

pub trait MidiDecoder {
    type Packet;

    fn feed(&mut self, packet: Self::Packet) -> Option<MidiMessage<'_>>;
    fn reset(&mut self);
}
