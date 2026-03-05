use super::types::MidiMessage;

// ---- Traits ----

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

// ---- DIN MIDI ----

pub struct DinMidiEncoder;

impl DinMidiEncoder {
    /// DIN MIDI is a byte stream, so we emit raw bytes rather than fixed packets.
    pub fn emit_bytes<S>(&mut self, message: &MidiMessage<'_>, sink: &mut S) -> Result<(), S::Error>
    where
        S: PacketSink<Packet = u8>,
    {
        match message {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => {
                sink.try_send(0x90 | (channel & 0x0F))?;
                sink.try_send(*note)?;
                sink.try_send(*velocity)?;
            }
            MidiMessage::NoteOff {
                channel,
                note,
                velocity,
            } => {
                sink.try_send(0x80 | (channel & 0x0F))?;
                sink.try_send(*note)?;
                sink.try_send(*velocity)?;
            }
            MidiMessage::ControlChange {
                channel,
                control,
                value,
            } => {
                sink.try_send(0xB0 | (channel & 0x0F))?;
                sink.try_send(*control)?;
                sink.try_send(*value)?;
            }
            MidiMessage::ProgramChange { channel, program } => {
                sink.try_send(0xC0 | (channel & 0x0F))?;
                sink.try_send(*program)?;
            }
            MidiMessage::PitchBend { channel, value } => {
                let u = (*value + 8192) as u16;
                sink.try_send(0xE0 | (channel & 0x0F))?;
                sink.try_send((u & 0x7F) as u8)?;
                sink.try_send(((u >> 7) & 0x7F) as u8)?;
            }
            MidiMessage::Sysex(data) => {
                for &b in *data {
                    sink.try_send(b)?;
                }
            }
        }
        Ok(())
    }
}

pub struct DinMidiDecoder<const SYSEX_N: usize> {
    status: u8,
    data: [u8; 2],
    count: u8,
    sysex_buf: [u8; SYSEX_N],
    sysex_len: usize,
    in_sysex: bool,
}

impl<const SYSEX_N: usize> DinMidiDecoder<SYSEX_N> {
    pub fn new() -> Self {
        Self {
            status: 0,
            data: [0; 2],
            count: 0,
            sysex_buf: [0; SYSEX_N],
            sysex_len: 0,
            in_sysex: false,
        }
    }

    fn push_sysex(&mut self, byte: u8) {
        if self.sysex_len < SYSEX_N {
            self.sysex_buf[self.sysex_len] = byte;
            self.sysex_len += 1;
        }
    }

    fn try_complete(&mut self) -> Option<MidiMessage<'_>> {
        let command = self.status & 0xF0;
        let channel = self.status & 0x0F;

        match (command, self.count) {
            (0x80, 2) => {
                self.count = 0;
                Some(MidiMessage::NoteOff {
                    channel,
                    note: self.data[0],
                    velocity: self.data[1],
                })
            }
            (0x90, 2) => {
                self.count = 0;
                let (note, velocity) = (self.data[0], self.data[1]);
                Some(if velocity == 0 {
                    MidiMessage::NoteOff {
                        channel,
                        note,
                        velocity: 0,
                    }
                } else {
                    MidiMessage::NoteOn {
                        channel,
                        note,
                        velocity,
                    }
                })
            }
            (0xB0, 2) => {
                self.count = 0;
                Some(MidiMessage::ControlChange {
                    channel,
                    control: self.data[0],
                    value: self.data[1],
                })
            }
            (0xC0, 1) => {
                self.count = 0;
                Some(MidiMessage::ProgramChange {
                    channel,
                    program: self.data[0],
                })
            }
            (0xE0, 2) => {
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

impl<const SYSEX_N: usize> MidiDecoder for DinMidiDecoder<SYSEX_N> {
    type Packet = u8;

    fn feed(&mut self, byte: u8) -> Option<MidiMessage<'_>> {
        if byte & 0x80 != 0 {
            if byte == 0xF0 {
                self.in_sysex = true;
                self.sysex_len = 0;
                self.push_sysex(byte);
                return None;
            }
            if byte == 0xF7 {
                self.push_sysex(byte);
                self.in_sysex = false;
                let len = self.sysex_len;
                self.sysex_len = 0;
                return Some(MidiMessage::Sysex(&self.sysex_buf[..len]));
            }
            self.in_sysex = false;
            self.status = byte;
            self.count = 0;
            return None;
        }

        if self.in_sysex {
            self.push_sysex(byte);
            return None;
        }

        if self.status == 0 {
            return None;
        }

        self.data[self.count as usize] = byte;
        self.count += 1;
        self.try_complete()
    }

    fn reset(&mut self) {
        self.status = 0;
        self.count = 0;
        self.sysex_len = 0;
        self.in_sysex = false;
    }
}

// ---- USB MIDI ----

pub struct UsbMidiEncoder;

impl MidiEncoder for UsbMidiEncoder {
    type Packet = [u8; 4];

    fn emit<S>(&mut self, message: &MidiMessage<'_>, sink: &mut S) -> Result<(), S::Error>
    where
        S: PacketSink<Packet = [u8; 4]>,
    {
        match message {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => sink.try_send([0x09, 0x90 | (channel & 0x0F), *note, *velocity])?,
            MidiMessage::NoteOff {
                channel,
                note,
                velocity,
            } => sink.try_send([0x08, 0x80 | (channel & 0x0F), *note, *velocity])?,
            MidiMessage::ControlChange {
                channel,
                control,
                value,
            } => sink.try_send([0x0B, 0xB0 | (channel & 0x0F), *control, *value])?,
            MidiMessage::ProgramChange { channel, program } => {
                sink.try_send([0x0C, 0xC0 | (channel & 0x0F), *program, 0x00])?
            }
            MidiMessage::PitchBend { channel, value } => {
                let u = (*value + 8192) as u16;
                sink.try_send([
                    0x0E,
                    0xE0 | (channel & 0x0F),
                    (u & 0x7F) as u8,
                    ((u >> 7) & 0x7F) as u8,
                ])?;
            }
            MidiMessage::Sysex(data) => {
                let mut i = 0;
                while i < data.len() {
                    let remaining = data.len() - i;
                    let packet = match remaining {
                        1 => [0x05, data[i], 0x00, 0x00],
                        2 => [0x06, data[i], data[i + 1], 0x00],
                        r if r >= 3 && i + 3 >= data.len() => {
                            [0x07, data[i], data[i + 1], data[i + 2]]
                        }
                        _ => [0x04, data[i], data[i + 1], data[i + 2]],
                    };
                    sink.try_send(packet)?;
                    i += 3;
                }
            }
        }
        Ok(())
    }
}

pub struct UsbMidiDecoder<const SYSEX_N: usize> {
    sysex_buf: [u8; SYSEX_N],
    sysex_len: usize,
    in_sysex: bool,
}

impl<const SYSEX_N: usize> UsbMidiDecoder<SYSEX_N> {
    pub fn new() -> Self {
        Self {
            sysex_buf: [0; SYSEX_N],
            sysex_len: 0,
            in_sysex: false,
        }
    }

    fn push_sysex(&mut self, byte: u8) {
        if self.sysex_len < SYSEX_N {
            self.sysex_buf[self.sysex_len] = byte;
            self.sysex_len += 1;
        }
    }
}

impl<const SYSEX_N: usize> MidiDecoder for UsbMidiDecoder<SYSEX_N> {
    type Packet = [u8; 4];

    fn feed(&mut self, packet: [u8; 4]) -> Option<MidiMessage<'_>> {
        let cin = packet[0] & 0x0F;
        let status = packet[1];
        let d1 = packet[2];
        let d2 = packet[3];
        let channel = status & 0x0F;

        match cin {
            0x04 => {
                self.in_sysex = true;
                self.push_sysex(packet[1]);
                self.push_sysex(packet[2]);
                self.push_sysex(packet[3]);
                None
            }
            0x05 | 0x06 | 0x07 => {
                let count = (cin - 0x04) as usize;
                for &b in &packet[1..=count] {
                    self.push_sysex(b);
                }
                self.in_sysex = false;
                let len = self.sysex_len;
                self.sysex_len = 0;
                Some(MidiMessage::Sysex(&self.sysex_buf[..len]))
            }
            0x08 => Some(MidiMessage::NoteOff {
                channel,
                note: d1,
                velocity: d2,
            }),
            0x09 => Some(if d2 == 0 {
                MidiMessage::NoteOff {
                    channel,
                    note: d1,
                    velocity: 0,
                }
            } else {
                MidiMessage::NoteOn {
                    channel,
                    note: d1,
                    velocity: d2,
                }
            }),
            0x0B => Some(MidiMessage::ControlChange {
                channel,
                control: d1,
                value: d2,
            }),
            0x0C => Some(MidiMessage::ProgramChange {
                channel,
                program: d1,
            }),
            0x0E => {
                let raw = (d1 as u16) | ((d2 as u16) << 7);
                Some(MidiMessage::PitchBend {
                    channel,
                    value: raw as i16 - 8192,
                })
            }
            _ => None,
        }
    }

    fn reset(&mut self) {
        self.sysex_len = 0;
        self.in_sysex = false;
    }
}

// ---- Embassy integration ----

#[cfg(feature = "embassy")]
mod embassy_impl {
    use super::PacketSink;
    use embassy_sync::blocking_mutex::raw::RawMutex;
    use embassy_sync::channel::Sender;

    impl<'ch, M, T, const N: usize> PacketSink for Sender<'ch, M, T, N>
    where
        M: RawMutex,
        T: 'ch,
    {
        type Packet = T;
        type Error = embassy_sync::channel::TrySendError<T>;

        fn try_send(&mut self, packet: T) -> Result<(), Self::Error> {
            Sender::try_send(self, packet)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Test helpers ----

    /// A simple Vec-like sink for testing, backed by a fixed-size array.
    struct CollectSink<T, const N: usize> {
        buf: [Option<T>; N],
        len: usize,
    }

    impl<T: Copy, const N: usize> CollectSink<T, N> {
        fn new() -> Self {
            Self {
                buf: [None; N],
                len: 0,
            }
        }

        fn packets(&self) -> &[Option<T>] {
            &self.buf[..self.len]
        }

        fn get(&self, i: usize) -> T {
            self.buf[i].unwrap()
        }

        fn len(&self) -> usize {
            self.len
        }
    }

    #[derive(Debug)]
    struct SinkFullError;

    impl<T: Copy, const N: usize> PacketSink for CollectSink<T, N> {
        type Packet = T;
        type Error = SinkFullError;

        fn try_send(&mut self, packet: T) -> Result<(), SinkFullError> {
            if self.len >= N {
                return Err(SinkFullError);
            }
            self.buf[self.len] = Some(packet);
            self.len += 1;
            Ok(())
        }
    }

    // ---- USB MIDI Encoder tests ----

    mod usb_encoder {
        use super::*;

        fn encode(message: &MidiMessage<'_>) -> CollectSink<[u8; 4], 16> {
            let mut encoder = UsbMidiEncoder;
            let mut sink = CollectSink::new();
            encoder.emit(message, &mut sink).unwrap();
            sink
        }

        #[test]
        fn note_on() {
            let sink = encode(&MidiMessage::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100,
            });
            assert_eq!(sink.len(), 1);
            assert_eq!(sink.get(0), [0x09, 0x90, 60, 100]);
        }

        #[test]
        fn note_on_channel() {
            let sink = encode(&MidiMessage::NoteOn {
                channel: 5,
                note: 60,
                velocity: 100,
            });
            assert_eq!(sink.get(0), [0x09, 0x95, 60, 100]);
        }

        #[test]
        fn note_on_channel_clamped() {
            // Channel nibble should be masked to 4 bits
            let sink = encode(&MidiMessage::NoteOn {
                channel: 0xFF,
                note: 60,
                velocity: 100,
            });
            assert_eq!(sink.get(0)[1] & 0x0F, 0x0F);
        }

        #[test]
        fn note_off() {
            let sink = encode(&MidiMessage::NoteOff {
                channel: 0,
                note: 60,
                velocity: 64,
            });
            assert_eq!(sink.len(), 1);
            assert_eq!(sink.get(0), [0x08, 0x80, 60, 64]);
        }

        #[test]
        fn control_change() {
            let sink = encode(&MidiMessage::ControlChange {
                channel: 2,
                control: 7,
                value: 127,
            });
            assert_eq!(sink.len(), 1);
            assert_eq!(sink.get(0), [0x0B, 0xB2, 7, 127]);
        }

        #[test]
        fn program_change() {
            let sink = encode(&MidiMessage::ProgramChange {
                channel: 0,
                program: 42,
            });
            assert_eq!(sink.len(), 1);
            assert_eq!(sink.get(0), [0x0C, 0xC0, 42, 0x00]);
        }

        #[test]
        fn pitch_bend_center() {
            let sink = encode(&MidiMessage::PitchBend {
                channel: 0,
                value: 0,
            });
            assert_eq!(sink.len(), 1);
            // Center = 8192, LSB = 0x00, MSB = 0x40
            assert_eq!(sink.get(0), [0x0E, 0xE0, 0x00, 0x40]);
        }

        #[test]
        fn pitch_bend_max() {
            let sink = encode(&MidiMessage::PitchBend {
                channel: 0,
                value: 8191,
            });
            assert_eq!(sink.len(), 1);
            assert_eq!(sink.get(0), [0x0E, 0xE0, 0x7F, 0x7F]);
        }

        #[test]
        fn pitch_bend_min() {
            let sink = encode(&MidiMessage::PitchBend {
                channel: 0,
                value: -8192,
            });
            assert_eq!(sink.len(), 1);
            assert_eq!(sink.get(0), [0x0E, 0xE0, 0x00, 0x00]);
        }

        #[test]
        fn sysex_single_packet() {
            // 3 data bytes + F0/F7 = fits in one end packet
            let data = [0xF0, 0x41, 0x10, 0xF7];
            let sink = encode(&MidiMessage::Sysex(&data));
            assert_eq!(sink.len(), 1);
            // 4 bytes: F0 + 2 data + F7 -> CIN 0x07 (sysex end, 3 bytes)
            assert_eq!(sink.get(0)[0] & 0x0F, 0x07);
        }

        #[test]
        fn sysex_multiple_packets() {
            // F0 + 6 data bytes + F7 = 8 bytes total
            // Packet 1: CIN 0x04, F0, d1, d2
            // Packet 2: CIN 0x04, d3, d4, d5
            // Packet 3: CIN 0x06, d6, F7, 0x00
            let data = [0xF0, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0xF7];
            let sink = encode(&MidiMessage::Sysex(&data));
            assert_eq!(sink.len(), 3);
            assert_eq!(sink.get(0)[0] & 0x0F, 0x04); // continue
            assert_eq!(sink.get(1)[0] & 0x0F, 0x04); // continue
            assert_eq!(sink.get(2)[0] & 0x0F, 0x06); // end, 2 bytes
        }

        #[test]
        fn sysex_end_1_byte() {
            // F0 + F7 = 2 bytes -> CIN 0x06 (end, 2 bytes: F0 and F7)
            // Actually F0 alone in packet 1 (CIN 0x04), then F7 alone (CIN 0x05)
            let data = [0xF0, 0xF7];
            let sink = encode(&MidiMessage::Sysex(&data));
            // Last packet CIN should be 0x05 (1 byte end)
            let last = sink.get(sink.len() - 1);
            assert_eq!(last[0] & 0x0F, 0x05);
        }
    }

    // ---- USB MIDI Decoder tests ----

    mod usb_decoder {
        use super::*;

        fn decode(packet: [u8; 4]) -> Option<MidiMessage<'static>> {
            // For non-sysex we can use a zero-size sysex buffer
            // We can't return borrowed MidiMessage from a local decoder,
            // so we match and reconstruct for non-sysex variants.
            let mut decoder = UsbMidiDecoder::<0>::new();
            match decoder.feed(packet) {
                Some(MidiMessage::NoteOn {
                    channel,
                    note,
                    velocity,
                }) => Some(MidiMessage::NoteOn {
                    channel,
                    note,
                    velocity,
                }),
                Some(MidiMessage::NoteOff {
                    channel,
                    note,
                    velocity,
                }) => Some(MidiMessage::NoteOff {
                    channel,
                    note,
                    velocity,
                }),
                Some(MidiMessage::ControlChange {
                    channel,
                    control,
                    value,
                }) => Some(MidiMessage::ControlChange {
                    channel,
                    control,
                    value,
                }),
                Some(MidiMessage::ProgramChange { channel, program }) => {
                    Some(MidiMessage::ProgramChange { channel, program })
                }
                Some(MidiMessage::PitchBend { channel, value }) => {
                    Some(MidiMessage::PitchBend { channel, value })
                }
                Some(MidiMessage::Sysex(_)) => panic!("unexpected sysex"),
                None => None,
            }
        }

        #[test]
        fn note_on() {
            let msg = decode([0x09, 0x92, 60, 100]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::NoteOn {
                    channel: 2,
                    note: 60,
                    velocity: 100
                }
            ));
        }

        #[test]
        fn note_on_velocity_zero_becomes_note_off() {
            let msg = decode([0x09, 0x90, 60, 0]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::NoteOff {
                    channel: 0,
                    note: 60,
                    velocity: 0
                }
            ));
        }

        #[test]
        fn note_off() {
            let msg = decode([0x08, 0x83, 48, 64]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::NoteOff {
                    channel: 3,
                    note: 48,
                    velocity: 64
                }
            ));
        }

        #[test]
        fn control_change() {
            let msg = decode([0x0B, 0xB1, 7, 127]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::ControlChange {
                    channel: 1,
                    control: 7,
                    value: 127
                }
            ));
        }

        #[test]
        fn program_change() {
            let msg = decode([0x0C, 0xC0, 42, 0x00]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::ProgramChange {
                    channel: 0,
                    program: 42
                }
            ));
        }

        #[test]
        fn pitch_bend_center() {
            let msg = decode([0x0E, 0xE0, 0x00, 0x40]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::PitchBend {
                    channel: 0,
                    value: 0
                }
            ));
        }

        #[test]
        fn pitch_bend_min() {
            let msg = decode([0x0E, 0xE0, 0x00, 0x00]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::PitchBend {
                    channel: 0,
                    value: -8192
                }
            ));
        }

        #[test]
        fn unknown_cin_returns_none() {
            assert!(decode([0x00, 0x00, 0x00, 0x00]).is_none());
            assert!(decode([0x01, 0x00, 0x00, 0x00]).is_none());
            assert!(decode([0x0F, 0x00, 0x00, 0x00]).is_none());
        }

        #[test]
        fn sysex_single_packet() {
            let mut decoder = UsbMidiDecoder::<64>::new();
            // CIN 0x07: sysex end, 3 bytes
            let result = decoder.feed([0x07, 0xF0, 0x41, 0xF7]);
            match result {
                Some(MidiMessage::Sysex(data)) => {
                    assert_eq!(data, &[0xF0, 0x41, 0xF7]);
                }
                _ => panic!("expected sysex"),
            }
        }

        #[test]
        fn sysex_multi_packet() {
            let mut decoder = UsbMidiDecoder::<64>::new();
            // Start/continue
            assert!(decoder.feed([0x04, 0xF0, 0x01, 0x02]).is_none());
            assert!(decoder.feed([0x04, 0x03, 0x04, 0x05]).is_none());
            // End with 2 bytes
            let result = decoder.feed([0x06, 0x06, 0xF7, 0x00]);
            match result {
                Some(MidiMessage::Sysex(data)) => {
                    assert_eq!(data, &[0xF0, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0xF7]);
                }
                _ => panic!("expected sysex"),
            }
        }

        #[test]
        fn reset_clears_sysex_state() {
            let mut decoder = UsbMidiDecoder::<64>::new();
            assert!(decoder.feed([0x04, 0xF0, 0x01, 0x02]).is_none());
            decoder.reset();
            // After reset, a standard message should decode normally
            let result = decoder.feed([0x09, 0x90, 60, 100]);
            assert!(matches!(result, Some(MidiMessage::NoteOn { .. })));
        }
    }

    // ---- USB MIDI round-trip tests ----

    mod usb_roundtrip {
        use super::*;

        fn roundtrip(message: &MidiMessage<'_>) -> Option<MidiMessage<'static>> {
            let mut encoder = UsbMidiEncoder;
            let mut sink = CollectSink::<[u8; 4], 16>::new();
            encoder.emit(message, &mut sink).unwrap();

            let mut decoder = UsbMidiDecoder::<256>::new();
            let mut result = None;
            for i in 0..sink.len() {
                if let Some(packet) = sink.buf[i] {
                    if let Some(msg) = decoder.feed(packet) {
                        result = Some(match msg {
                            MidiMessage::NoteOn {
                                channel,
                                note,
                                velocity,
                            } => MidiMessage::NoteOn {
                                channel,
                                note,
                                velocity,
                            },
                            MidiMessage::NoteOff {
                                channel,
                                note,
                                velocity,
                            } => MidiMessage::NoteOff {
                                channel,
                                note,
                                velocity,
                            },
                            MidiMessage::ControlChange {
                                channel,
                                control,
                                value,
                            } => MidiMessage::ControlChange {
                                channel,
                                control,
                                value,
                            },
                            MidiMessage::ProgramChange { channel, program } => {
                                MidiMessage::ProgramChange { channel, program }
                            }
                            MidiMessage::PitchBend { channel, value } => {
                                MidiMessage::PitchBend { channel, value }
                            }
                            MidiMessage::Sysex(_) => panic!("use sysex_roundtrip"),
                        });
                    }
                }
            }
            result
        }

        #[test]
        fn note_on_roundtrip() {
            let msg = MidiMessage::NoteOn {
                channel: 3,
                note: 64,
                velocity: 80,
            };
            let result = roundtrip(&msg).unwrap();
            assert!(matches!(
                result,
                MidiMessage::NoteOn {
                    channel: 3,
                    note: 64,
                    velocity: 80
                }
            ));
        }

        #[test]
        fn note_off_roundtrip() {
            let msg = MidiMessage::NoteOff {
                channel: 1,
                note: 48,
                velocity: 0,
            };
            let result = roundtrip(&msg).unwrap();
            assert!(matches!(
                result,
                MidiMessage::NoteOff {
                    channel: 1,
                    note: 48,
                    velocity: 0
                }
            ));
        }

        #[test]
        fn control_change_roundtrip() {
            let msg = MidiMessage::ControlChange {
                channel: 0,
                control: 74,
                value: 64,
            };
            let result = roundtrip(&msg).unwrap();
            assert!(matches!(
                result,
                MidiMessage::ControlChange {
                    channel: 0,
                    control: 74,
                    value: 64
                }
            ));
        }

        #[test]
        fn program_change_roundtrip() {
            let msg = MidiMessage::ProgramChange {
                channel: 2,
                program: 10,
            };
            let result = roundtrip(&msg).unwrap();
            assert!(matches!(
                result,
                MidiMessage::ProgramChange {
                    channel: 2,
                    program: 10
                }
            ));
        }

        #[test]
        fn pitch_bend_roundtrip_center() {
            let msg = MidiMessage::PitchBend {
                channel: 0,
                value: 0,
            };
            let result = roundtrip(&msg).unwrap();
            assert!(matches!(
                result,
                MidiMessage::PitchBend {
                    channel: 0,
                    value: 0
                }
            ));
        }

        #[test]
        fn pitch_bend_roundtrip_max() {
            let msg = MidiMessage::PitchBend {
                channel: 0,
                value: 8191,
            };
            let result = roundtrip(&msg).unwrap();
            assert!(matches!(
                result,
                MidiMessage::PitchBend {
                    channel: 0,
                    value: 8191
                }
            ));
        }

        #[test]
        fn pitch_bend_roundtrip_min() {
            let msg = MidiMessage::PitchBend {
                channel: 0,
                value: -8192,
            };
            let result = roundtrip(&msg).unwrap();
            assert!(matches!(
                result,
                MidiMessage::PitchBend {
                    channel: 0,
                    value: -8192
                }
            ));
        }

        #[test]
        fn sysex_roundtrip() {
            let data = [0xF0, 0x41, 0x10, 0x42, 0x12, 0x01, 0x02, 0x03, 0xF7];
            let msg = MidiMessage::Sysex(&data);

            let mut encoder = UsbMidiEncoder;
            let mut sink = CollectSink::<[u8; 4], 16>::new();
            encoder.emit(&msg, &mut sink).unwrap();

            let mut decoder = UsbMidiDecoder::<64>::new();
            let mut result_data: Option<&[u8]> = None;

            // We need the decoder to outlive the loop so we can inspect sysex data
            // so we drive it manually here
            for i in 0..sink.len() {
                if let Some(packet) = sink.buf[i] {
                    if let Some(MidiMessage::Sysex(decoded)) = decoder.feed(packet) {
                        assert_eq!(decoded, &data);
                        return; // success
                    }
                }
            }
            panic!("no sysex message produced");
        }
    }

    // ---- DIN MIDI Encoder tests ----

    mod din_encoder {
        use super::*;

        fn encode(message: &MidiMessage<'_>) -> CollectSink<u8, 16> {
            let mut encoder = DinMidiEncoder;
            let mut sink = CollectSink::new();
            encoder.emit_bytes(message, &mut sink).unwrap();
            sink
        }

        #[test]
        fn note_on() {
            let sink = encode(&MidiMessage::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100,
            });
            assert_eq!(sink.len(), 3);
            assert_eq!(sink.get(0), 0x90);
            assert_eq!(sink.get(1), 60);
            assert_eq!(sink.get(2), 100);
        }

        #[test]
        fn note_on_channel() {
            let sink = encode(&MidiMessage::NoteOn {
                channel: 5,
                note: 60,
                velocity: 100,
            });
            assert_eq!(sink.get(0), 0x95);
        }

        #[test]
        fn note_off() {
            let sink = encode(&MidiMessage::NoteOff {
                channel: 1,
                note: 48,
                velocity: 64,
            });
            assert_eq!(sink.len(), 3);
            assert_eq!(sink.get(0), 0x81);
            assert_eq!(sink.get(1), 48);
            assert_eq!(sink.get(2), 64);
        }

        #[test]
        fn control_change() {
            let sink = encode(&MidiMessage::ControlChange {
                channel: 0,
                control: 7,
                value: 127,
            });
            assert_eq!(sink.len(), 3);
            assert_eq!(sink.get(0), 0xB0);
            assert_eq!(sink.get(1), 7);
            assert_eq!(sink.get(2), 127);
        }

        #[test]
        fn program_change() {
            let sink = encode(&MidiMessage::ProgramChange {
                channel: 0,
                program: 10,
            });
            assert_eq!(sink.len(), 2);
            assert_eq!(sink.get(0), 0xC0);
            assert_eq!(sink.get(1), 10);
        }

        #[test]
        fn pitch_bend_center() {
            let sink = encode(&MidiMessage::PitchBend {
                channel: 0,
                value: 0,
            });
            assert_eq!(sink.len(), 3);
            assert_eq!(sink.get(0), 0xE0);
            assert_eq!(sink.get(1), 0x00);
            assert_eq!(sink.get(2), 0x40);
        }

        #[test]
        fn sysex() {
            let data = [0xF0, 0x41, 0x10, 0xF7];
            let sink = encode(&MidiMessage::Sysex(&data));
            assert_eq!(sink.len(), 4);
            assert_eq!(sink.get(0), 0xF0);
            assert_eq!(sink.get(1), 0x41);
            assert_eq!(sink.get(2), 0x10);
            assert_eq!(sink.get(3), 0xF7);
        }
    }

    // ---- DIN MIDI Decoder tests ----

    mod din_decoder {
        use super::*;

        fn feed_all<'d, const N: usize>(
            decoder: &'d mut DinMidiDecoder<N>,
            bytes: &[u8],
        ) -> Option<MidiMessage<'d>> {
            let mut result = None;
            for &b in bytes {
                if let Some(msg) = decoder.feed(b) {
                    result = Some(msg);
                }
            }
            result
        }

        #[test]
        fn note_on() {
            let mut decoder = DinMidiDecoder::<0>::new();
            assert!(decoder.feed(0x90).is_none());
            assert!(decoder.feed(60).is_none());
            let msg = decoder.feed(100).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::NoteOn {
                    channel: 0,
                    note: 60,
                    velocity: 100
                }
            ));
        }

        #[test]
        fn note_on_velocity_zero_becomes_note_off() {
            let mut decoder = DinMidiDecoder::<0>::new();
            let msg = feed_all(&mut decoder, &[0x90, 60, 0]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::NoteOff {
                    channel: 0,
                    note: 60,
                    velocity: 0
                }
            ));
        }

        #[test]
        fn note_off() {
            let mut decoder = DinMidiDecoder::<0>::new();
            let msg = feed_all(&mut decoder, &[0x83, 48, 64]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::NoteOff {
                    channel: 3,
                    note: 48,
                    velocity: 64
                }
            ));
        }

        #[test]
        fn control_change() {
            let mut decoder = DinMidiDecoder::<0>::new();
            let msg = feed_all(&mut decoder, &[0xB2, 7, 127]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::ControlChange {
                    channel: 2,
                    control: 7,
                    value: 127
                }
            ));
        }

        #[test]
        fn program_change() {
            let mut decoder = DinMidiDecoder::<0>::new();
            let msg = feed_all(&mut decoder, &[0xC0, 42]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::ProgramChange {
                    channel: 0,
                    program: 42
                }
            ));
        }

        #[test]
        fn pitch_bend_center() {
            let mut decoder = DinMidiDecoder::<0>::new();
            let msg = feed_all(&mut decoder, &[0xE0, 0x00, 0x40]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::PitchBend {
                    channel: 0,
                    value: 0
                }
            ));
        }

        #[test]
        fn pitch_bend_min() {
            let mut decoder = DinMidiDecoder::<0>::new();
            let msg = feed_all(&mut decoder, &[0xE0, 0x00, 0x00]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::PitchBend {
                    channel: 0,
                    value: -8192
                }
            ));
        }

        #[test]
        fn pitch_bend_max() {
            let mut decoder = DinMidiDecoder::<0>::new();
            let msg = feed_all(&mut decoder, &[0xE0, 0x7F, 0x7F]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::PitchBend {
                    channel: 0,
                    value: 8191
                }
            ));
        }

        #[test]
        fn running_status() {
            // After a NoteOn status, subsequent data bytes reuse it
            let mut decoder = DinMidiDecoder::<0>::new();
            // First message
            let msg1 = feed_all(&mut decoder, &[0x90, 60, 100]).unwrap();
            assert!(matches!(
                msg1,
                MidiMessage::NoteOn {
                    note: 60,
                    velocity: 100,
                    ..
                }
            ));
            // Second message reuses running status
            let msg2 = feed_all(&mut decoder, &[64, 80]).unwrap();
            assert!(matches!(
                msg2,
                MidiMessage::NoteOn {
                    note: 64,
                    velocity: 80,
                    ..
                }
            ));
        }

        #[test]
        fn data_before_status_ignored() {
            let mut decoder = DinMidiDecoder::<0>::new();
            // Data bytes before any status should be ignored
            assert!(decoder.feed(0x60).is_none());
            assert!(decoder.feed(0x40).is_none());
            // Now a proper message
            let msg = feed_all(&mut decoder, &[0x90, 60, 100]).unwrap();
            assert!(matches!(msg, MidiMessage::NoteOn { .. }));
        }

        #[test]
        fn status_byte_resets_parser() {
            let mut decoder = DinMidiDecoder::<0>::new();
            // Start a NoteOn but interrupt with a new status
            assert!(decoder.feed(0x90).is_none());
            assert!(decoder.feed(60).is_none());
            // Interrupt with NoteOff status before second data byte
            assert!(decoder.feed(0x80).is_none());
            // Now complete the NoteOff
            let msg = feed_all(&mut decoder, &[48, 64]).unwrap();
            assert!(matches!(
                msg,
                MidiMessage::NoteOff {
                    note: 48,
                    velocity: 64,
                    ..
                }
            ));
        }

        #[test]
        fn sysex() {
            let mut decoder = DinMidiDecoder::<64>::new();
            let data = [0xF0, 0x41, 0x10, 0x42, 0xF7];
            let msg = feed_all(&mut decoder, &data).unwrap();
            match msg {
                MidiMessage::Sysex(decoded) => assert_eq!(decoded, &data),
                _ => panic!("expected sysex"),
            }
        }

        #[test]
        fn sysex_then_normal_message() {
            let mut decoder = DinMidiDecoder::<64>::new();
            let sysex = [0xF0, 0x01, 0x02, 0xF7];
            feed_all(&mut decoder, &sysex);
            // After sysex, normal messages should work
            let msg = feed_all(&mut decoder, &[0x90, 60, 100]).unwrap();
            assert!(matches!(msg, MidiMessage::NoteOn { .. }));
        }

        #[test]
        fn sysex_overflow_is_silent() {
            // Buffer of 4 bytes, sysex of 8 — should not panic, just truncate
            let mut decoder = DinMidiDecoder::<4>::new();
            let data = [0xF0, 0x01, 0x02, 0x03, 0x04, 0x05, 0xF7];
            // Should not panic
            feed_all(&mut decoder, &data);
        }

        #[test]
        fn reset_clears_state() {
            let mut decoder = DinMidiDecoder::<0>::new();
            assert!(decoder.feed(0x90).is_none());
            assert!(decoder.feed(60).is_none());
            decoder.reset();
            // Data bytes after reset should be ignored (no status)
            assert!(decoder.feed(100).is_none());
        }

        #[test]
        fn multiple_messages_sequential() {
            let mut decoder = DinMidiDecoder::<0>::new();
            let msg1 = feed_all(&mut decoder, &[0x90, 60, 100]).unwrap();
            let msg2 = feed_all(&mut decoder, &[0x80, 60, 0]).unwrap();
            let msg3 = feed_all(&mut decoder, &[0xB0, 7, 64]).unwrap();
            assert!(matches!(msg1, MidiMessage::NoteOn { note: 60, .. }));
            assert!(matches!(msg2, MidiMessage::NoteOff { note: 60, .. }));
            assert!(matches!(
                msg3,
                MidiMessage::ControlChange {
                    control: 7,
                    value: 64,
                    ..
                }
            ));
        }
    }

    // ---- DIN MIDI round-trip tests ----

    mod din_roundtrip {
        use super::*;

        fn roundtrip_non_sysex(message: &MidiMessage<'_>) -> Option<MidiMessage<'static>> {
            let mut encoder = DinMidiEncoder;
            let mut sink = CollectSink::<u8, 16>::new();
            encoder.emit_bytes(message, &mut sink).unwrap();

            let mut decoder = DinMidiDecoder::<0>::new();
            let mut result = None;
            for i in 0..sink.len() {
                if let Some(byte) = sink.buf[i] {
                    if let Some(msg) = decoder.feed(byte) {
                        result = Some(match msg {
                            MidiMessage::NoteOn {
                                channel,
                                note,
                                velocity,
                            } => MidiMessage::NoteOn {
                                channel,
                                note,
                                velocity,
                            },
                            MidiMessage::NoteOff {
                                channel,
                                note,
                                velocity,
                            } => MidiMessage::NoteOff {
                                channel,
                                note,
                                velocity,
                            },
                            MidiMessage::ControlChange {
                                channel,
                                control,
                                value,
                            } => MidiMessage::ControlChange {
                                channel,
                                control,
                                value,
                            },
                            MidiMessage::ProgramChange { channel, program } => {
                                MidiMessage::ProgramChange { channel, program }
                            }
                            MidiMessage::PitchBend { channel, value } => {
                                MidiMessage::PitchBend { channel, value }
                            }
                            MidiMessage::Sysex(_) => panic!("unexpected sysex"),
                        });
                    }
                }
            }
            result
        }

        #[test]
        fn note_on_roundtrip() {
            let msg = MidiMessage::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100,
            };
            let result = roundtrip_non_sysex(&msg).unwrap();
            assert!(matches!(
                result,
                MidiMessage::NoteOn {
                    channel: 0,
                    note: 60,
                    velocity: 100
                }
            ));
        }

        #[test]
        fn note_off_roundtrip() {
            let msg = MidiMessage::NoteOff {
                channel: 1,
                note: 48,
                velocity: 64,
            };
            let result = roundtrip_non_sysex(&msg).unwrap();
            assert!(matches!(
                result,
                MidiMessage::NoteOff {
                    channel: 1,
                    note: 48,
                    velocity: 64
                }
            ));
        }

        #[test]
        fn control_change_roundtrip() {
            let msg = MidiMessage::ControlChange {
                channel: 3,
                control: 7,
                value: 127,
            };
            let result = roundtrip_non_sysex(&msg).unwrap();
            assert!(matches!(
                result,
                MidiMessage::ControlChange {
                    channel: 3,
                    control: 7,
                    value: 127
                }
            ));
        }

        #[test]
        fn program_change_roundtrip() {
            let msg = MidiMessage::ProgramChange {
                channel: 0,
                program: 42,
            };
            let result = roundtrip_non_sysex(&msg).unwrap();
            assert!(matches!(
                result,
                MidiMessage::ProgramChange {
                    channel: 0,
                    program: 42
                }
            ));
        }

        #[test]
        fn pitch_bend_roundtrip_values() {
            for value in [-8192i16, -4096, 0, 4096, 8191] {
                let msg = MidiMessage::PitchBend { channel: 0, value };
                let result = roundtrip_non_sysex(&msg).unwrap();
                match result {
                    MidiMessage::PitchBend { value: v, .. } => assert_eq!(v, value),
                    _ => panic!("expected pitch bend"),
                }
            }
        }

        #[test]
        fn sysex_roundtrip() {
            let data = [0xF0, 0x41, 0x10, 0x42, 0x12, 0xF7];
            let msg = MidiMessage::Sysex(&data);

            let mut encoder = DinMidiEncoder;
            let mut sink = CollectSink::<u8, 16>::new();
            encoder.emit_bytes(&msg, &mut sink).unwrap();

            let mut decoder = DinMidiDecoder::<64>::new();
            for i in 0..sink.len() {
                if let Some(byte) = sink.buf[i] {
                    if let Some(MidiMessage::Sysex(decoded)) = decoder.feed(byte) {
                        assert_eq!(decoded, &data);
                        return;
                    }
                }
            }
            panic!("no sysex produced");
        }

        #[test]
        fn all_channels_roundtrip() {
            for ch in 0u8..16 {
                let msg = MidiMessage::NoteOn {
                    channel: ch,
                    note: 60,
                    velocity: 100,
                };
                let result = roundtrip_non_sysex(&msg).unwrap();
                match result {
                    MidiMessage::NoteOn { channel, .. } => assert_eq!(channel, ch),
                    _ => panic!("expected note on"),
                }
            }
        }
    }
}
