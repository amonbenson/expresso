use defmt::warn;
use embassy_futures::select::{Either, select};
use embassy_stm32::{
    mode::Async,
    usart::{Uart, UartRx, UartTx},
};

use crate::midi::{MidiMessage, MidiMessageReceiver, MidiMessageSender};

#[embassy_executor::task]
pub async fn task(
    uart: Uart<'static, Async>,
    midi_out: MidiMessageReceiver<'static>,
    midi_in: MidiMessageSender<'static>,
) {
    let (tx, rx) = uart.split();

    match select(rx_loop(rx, &midi_in), tx_loop(tx, &midi_out)).await {
        Either::First(_) => warn!("DIN MIDI RX loop exited"),
        Either::Second(_) => warn!("DIN MIDI TX loop exited"),
    }
}

async fn rx_loop(mut rx: UartRx<'static, Async>, to_bus: &MidiMessageSender<'static>) {
    let mut buf = [0u8; 64];
    let mut parser = DinMidiParser::new();

    loop {
        match rx.read_until_idle(&mut buf).await {
            Ok(len) => {
                for &byte in &buf[..len] {
                    if let Some(message) = parser.feed(byte) {
                        if to_bus.try_send(message).is_err() {
                            warn!("DIN MIDI RX: bus full, dropped");
                        }
                    }
                }
            }
            Err(_) => {
                parser = DinMidiParser::new();
            }
        }
    }
}

async fn tx_loop(mut tx: UartTx<'static, Async>, from_router: &MidiMessageReceiver<'static>) {
    loop {
        let message = from_router.receive().await;
        let (len, bytes) = message_to_bytes(message);
        if tx.write(&bytes[..len]).await.is_err() {
            warn!("DIN MIDI TX: write error");
        }
    }
}

fn message_to_bytes(message: MidiMessage) -> (usize, [u8; 3]) {
    match message {
        MidiMessage::NoteOn {
            channel,
            note,
            velocity,
        } => (3, [0x90 | (channel & 0x0F), note, velocity]),
        MidiMessage::NoteOff {
            channel,
            note,
            velocity,
        } => (3, [0x80 | (channel & 0x0F), note, velocity]),
        MidiMessage::ControlChange {
            channel,
            control,
            value,
        } => (3, [0xB0 | (channel & 0x0F), control, value]),
        MidiMessage::ProgramChange { channel, program } => {
            (2, [0xC0 | (channel & 0x0F), program, 0x00])
        }
        MidiMessage::PitchBend { channel, value } => {
            let u = (value + 8192) as u16;
            (
                3,
                [
                    0xE0 | (channel & 0x0F),
                    (u & 0x7F) as u8,
                    ((u >> 7) & 0x7F) as u8,
                ],
            )
        }
        MidiMessage::ActiveSensing => (1, [0xFE, 0x00, 0x00]),
        MidiMessage::TimingClock => (1, [0xF8, 0x00, 0x00]),
    }
}

// Parses a stream of raw MIDI bytes into MidiMessage values.
// Handles running status and real-time messages interleaved anywhere in the stream.
struct DinMidiParser {
    status: u8,
    data: [u8; 2],
    count: u8,
}

impl DinMidiParser {
    fn new() -> Self {
        Self {
            status: 0,
            data: [0; 2],
            count: 0,
        }
    }

    fn feed(&mut self, byte: u8) -> Option<MidiMessage> {
        if byte & 0x80 != 0 {
            // Real-time messages (0xF8-0xFF) can appear anywhere and have no data bytes.
            match byte {
                0xF8 => return Some(MidiMessage::TimingClock),
                0xFE => return Some(MidiMessage::ActiveSensing),
                _ => {}
            }
            // Any other status byte resets the parser.
            self.status = byte;
            self.count = 0;
            None
        } else {
            // Data byte -- ignore if we haven't seen a valid status yet.
            if self.status == 0 {
                return None;
            }
            self.data[self.count as usize] = byte;
            self.count += 1;
            self.try_complete()
        }
    }

    fn try_complete(&mut self) -> Option<MidiMessage> {
        let command = self.status & 0xF0;
        let channel = self.status & 0x0F;

        match command {
            0x80 if self.count == 2 => {
                self.count = 0;
                Some(MidiMessage::NoteOff {
                    channel,
                    note: self.data[0],
                    velocity: self.data[1],
                })
            }
            0x90 if self.count == 2 => {
                self.count = 0;
                let (note, velocity) = (self.data[0], self.data[1]);
                // NoteOn with velocity 0 is equivalent to NoteOff (running status convention).
                if velocity == 0 {
                    Some(MidiMessage::NoteOff {
                        channel,
                        note,
                        velocity: 0,
                    })
                } else {
                    Some(MidiMessage::NoteOn {
                        channel,
                        note,
                        velocity,
                    })
                }
            }
            0xB0 if self.count == 2 => {
                self.count = 0;
                Some(MidiMessage::ControlChange {
                    channel,
                    control: self.data[0],
                    value: self.data[1],
                })
            }
            0xC0 if self.count == 1 => {
                self.count = 0;
                Some(MidiMessage::ProgramChange {
                    channel,
                    program: self.data[0],
                })
            }
            0xE0 if self.count == 2 => {
                self.count = 0;
                let raw = (self.data[0] as u16) | ((self.data[1] as u16) << 7);
                Some(MidiMessage::PitchBend {
                    channel,
                    value: raw as i16 - 8192,
                })
            }
            _ => None,
        }
    }
}
