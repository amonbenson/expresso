use snafu::Snafu;

use crate::component::Component;
use crate::midi::types::MidiEndpoint;
use crate::midi::{
    DinMidiDecoder, DinMidiEncoder, MidiDecoder, MidiEncoder, MidiMessageSink, PacketSink,
    UsbMidiDecoder, UsbMidiEncoder,
};

#[derive(Debug, Snafu)]
pub enum ConnectorError {}

pub struct Connector<E, D, P>
where
    E: MidiEncoder,
    D: MidiDecoder<Packet = E::Packet>,
    P: PacketSink<Packet = E::Packet>,
{
    endpoint: MidiEndpoint,
    encoder: E,
    decoder: D,
    packet_sink: P,
}

impl<E, D, P> Connector<E, D, P>
where
    E: MidiEncoder,
    D: MidiDecoder<Packet = E::Packet>,
    P: PacketSink<Packet = E::Packet>,
{
    pub fn new(endpoint: MidiEndpoint, encoder: E, decoder: D, packet_sink: P) -> Self {
        Self {
            endpoint,
            encoder,
            decoder,
            packet_sink,
        }
    }
}

impl<const C: usize, S, E, D, P> Component<C, S> for Connector<E, D, P>
where
    S: MidiMessageSink,
    E: MidiEncoder,
    D: MidiDecoder<Packet = E::Packet>,
    P: PacketSink<Packet = E::Packet>,
{
    type ProcessInputs = D::Packet;
    type Error = ConnectorError;

    fn handle_message(
        &mut self,
        message: crate::midi::MidiMessage,
        _source: MidiEndpoint,
        _sink: &mut S,
        _settings: &mut crate::settings::Settings<C>,
    ) -> Result<(), ConnectorError> {
        let _ = self.encoder.emit(&message, &mut self.packet_sink);
        Ok(())
    }

    fn process(
        &mut self,
        packet: Self::ProcessInputs,
        sink: &mut S,
        _settings: &mut crate::settings::Settings<C>,
    ) -> Result<(), ConnectorError> {
        if let Some(message) = self.decoder.feed(packet) {
            sink.emit(message, self.endpoint.into());
        }
        Ok(())
    }
}

// ---- Concrete connector types ----

pub type UsbConnector<const SYSEX_N: usize, P> =
    Connector<UsbMidiEncoder, UsbMidiDecoder<SYSEX_N>, P>;

pub type DinConnector<const SYSEX_N: usize, P> =
    Connector<DinMidiEncoder, DinMidiDecoder<SYSEX_N>, P>;

impl<const SYSEX_N: usize, P> Connector<UsbMidiEncoder, UsbMidiDecoder<SYSEX_N>, P>
where
    P: PacketSink<Packet = [u8; 4]>,
{
    pub fn usb(packet_sink: P) -> Self {
        Self::new(
            MidiEndpoint::Usb,
            UsbMidiEncoder,
            UsbMidiDecoder::new(),
            packet_sink,
        )
    }
}

impl<const SYSEX_N: usize, P> Connector<DinMidiEncoder, DinMidiDecoder<SYSEX_N>, P>
where
    P: PacketSink<Packet = u8>,
{
    pub fn din(packet_sink: P) -> Self {
        Self::new(
            MidiEndpoint::Din,
            DinMidiEncoder,
            DinMidiDecoder::new(),
            packet_sink,
        )
    }
}
