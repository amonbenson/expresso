use embassy_sync::{blocking_mutex::raw::RawMutex, channel::Receiver};

pub type Channel = u8;
pub type Note = u8;
pub type Velocity = u8;
pub type Control = u8;
pub type Program = u8;
pub type Value = u8;
pub type Value14 = u16;

#[derive(Debug, PartialEq)]
pub enum MidiMessage {
    NoteOff(Channel, Note, Velocity),
    NoteOn(Channel, Note, Velocity),
    PolyKeyPressure(Channel, Note, Value),
    ControlChange(Channel, Control, Value),
    ProgramChange(Channel, Program),
    ChannelPressure(Channel, Value),
    PitchBend(Channel, Value14),
}

impl MidiMessage {
    // pub fn from_din_packet(packet: [u8; 3]) -> Option<Self> {
    //     let status = packet[0];
    //     let command = status & 0xf0;
    //     let channel = status & 0x0f;
    //     let data1 = packet[1];
    //     let data2 = packet[2];

    //     match command {
    //         0x80 => MidiMessage::NoteOff(channel, data1, data2).into(),
    //         0x90 => MidiMessage::NoteOn(channel, data1, data2).into(),
    //         0xA0 => MidiMessage::PolyKeyPressure(channel, data1, data2).into(),
    //         0xB0 => MidiMessage::ControlChange(channel, data1, data2).into(),
    //         0xC0 => MidiMessage::ProgramChange(channel, data1).into(),
    //         0xD0 => MidiMessage::ChannelPressure(channel, data1).into(),
    //         0xE0 => MidiMessage::PitchBend(channel, (data1 as u16) | (data2 as u16) << 7).into(),
    //         _ => None
    //     }
    // }

    // pub fn from_usb_packet(packet: [u8; 4]) -> Option<Self> {
    //     // See: https://www.usb.org/sites/default/files/midi10.pdf
    //     let _cn = packet[0] >> 4;
    //     let _cin = packet[0] & 0x0f;

    //     match cin {
    //         0x08 =>
    //     }
    // }

    pub async fn receive_din<ReceiveFn, ReceiveFuture>(receive: ReceiveFn) -> Option<MidiMessage>
    where
        ReceiveFn: Fn() -> ReceiveFuture,
        ReceiveFuture: Future<Output = u8>,
    {
        // receive the command and channel byte
        let status = receive().await;
        let command = status & 0xf0;
        let channel = status & 0x0f;

        // receive the rest of the message depending on the command type
        match command {
            0x80 => MidiMessage::NoteOff(channel, receive().await, receive().await).into(),
            0x90 => MidiMessage::NoteOn(channel, receive().await, receive().await).into(),
            0xA0 => MidiMessage::PolyKeyPressure(channel, receive().await, receive().await).into(),
            0xB0 => MidiMessage::ControlChange(channel, receive().await, receive().await).into(),
            0xC0 => MidiMessage::ProgramChange(channel, receive().await).into(),
            0xD0 => MidiMessage::ChannelPressure(channel, receive().await).into(),
            0xE0 => MidiMessage::PitchBend(
                channel,
                (receive().await as u16) | (receive().await as u16) << 7,
            )
            .into(),
            _ => None,
        }
    }

    pub async fn receive_din_from_channel<'ch, M, const N: usize>(
        receiver: Receiver<'ch, M, u8, N>,
    ) -> Option<Self>
    where
        M: RawMutex,
    {
        Self::receive_din(|| receiver.receive()).await
    }

    pub async fn receive_usb<ReceiveFn, ReceiveFuture>(receive: ReceiveFn) -> Option<MidiMessage>
    where
        ReceiveFn: Fn() -> ReceiveFuture,
        ReceiveFuture: Future<Output = u8>,
    {
        // Documentation: https://www.usb.org/sites/default/files/midi10.pdf
        // Ignore cable number for now. We will also ignore the code index number for short messages, as the
        // Message type can be derived from the status code in `receive_din`
        let cin = receive().await & 0x0f;
        match cin {
            0x08..=0x0E => Self::receive_din(receive).await,
            _ => None,
        }
    }

    pub async fn receive_usb_from_channel<'ch, M, const N: usize>(
        receiver: Receiver<'ch, M, u8, N>,
    ) -> Option<Self>
    where
        M: RawMutex,
    {
        Self::receive_usb(|| receiver.receive()).await
    }

    pub async fn send_din<SendFn, SendFuture>(self, send: SendFn)
    where
        SendFn: Fn(u8) -> SendFuture,
        SendFuture: Future<Output = ()>,
    {
        match self {
            MidiMessage::NoteOff(channel, data1, data2) => {
                send(0x80 | channel).await;
                send(data1).await;
                send(data2).await
            }
            MidiMessage::NoteOn(channel, data1, data2) => {
                send(0x90 | channel).await;
                send(data1).await;
                send(data2).await
            }
            MidiMessage::PolyKeyPressure(channel, data1, data2) => {
                send(0xA0 | channel).await;
                send(data1).await;
                send(data2).await
            }
            MidiMessage::ControlChange(channel, data1, data2) => {
                send(0xB0 | channel).await;
                send(data1).await;
                send(data2).await
            }
            MidiMessage::ProgramChange(channel, data1) => {
                send(0xC0 | channel).await;
                send(data1).await
            }
            MidiMessage::ChannelPressure(channel, data1) => {
                send(0xD0 | channel).await;
                send(data1).await
            }
            MidiMessage::PitchBend(channel, data12) => {
                send(0xE0 | channel).await;
                send((data12 & 0x7f) as u8).await;
                send(((data12 >> 7) & 0x7f) as u8).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Channel};
    use futures::future::join_all;

    use super::*;

    #[test]
    fn test_receive_din() {
        futures::executor::block_on(async {
            let channel = Channel::<NoopRawMutex, u8, 8>::new();
            let receiver = channel.receiver();

            join_all([0x85, 0x12, 0x34].map(|b| channel.send(b))).await;
            let msg = MidiMessage::receive_din_from_channel(receiver).await;
            assert_eq!(msg, Some(MidiMessage::NoteOff(0x05, 0x12, 0x34)));
        })
    }
}
