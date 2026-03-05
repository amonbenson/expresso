use super::super::types::MidiMessage;
use super::traits::{MidiDecoder, MidiEncoder, PacketSink};

// ---- USB MIDI Encoder ----

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

// ---- USB MIDI Decoder ----

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

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;

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
            // F0 + 1 data byte + F7 = 3 bytes, fits in one CIN 0x07 end packet
            let data = [0xF0, 0x41, 0xF7];
            let sink = encode(&MidiMessage::Sysex(&data));
            assert_eq!(sink.len(), 1);
            assert_eq!(sink.get(0)[0] & 0x0F, 0x07);
        }

        #[test]
        fn sysex_multiple_packets() {
            // F0 + 6 data bytes + F7 = 8 bytes total
            let data = [0xF0, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0xF7];
            let sink = encode(&MidiMessage::Sysex(&data));
            assert_eq!(sink.len(), 3);
            assert_eq!(sink.get(0)[0] & 0x0F, 0x04);
            assert_eq!(sink.get(1)[0] & 0x0F, 0x04);
            assert_eq!(sink.get(2)[0] & 0x0F, 0x06);
        }

        #[test]
        fn sysex_end_1_byte() {
            // F0 + 2 data bytes + F7 = 4 bytes; first chunk is CIN 0x04, last chunk is F7 alone (CIN 0x05)
            let data = [0xF0, 0x41, 0x10, 0xF7];
            let sink = encode(&MidiMessage::Sysex(&data));
            let last = sink.get(sink.len() - 1);
            assert_eq!(last[0] & 0x0F, 0x05);
        }
    }

    mod usb_decoder {
        use super::*;

        fn decode(packet: [u8; 4]) -> Option<MidiMessage<'static>> {
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
            let result = decoder.feed([0x07, 0xF0, 0x41, 0xF7]);
            match result {
                Some(MidiMessage::Sysex(data)) => assert_eq!(data, &[0xF0, 0x41, 0xF7]),
                _ => panic!("expected sysex"),
            }
        }

        #[test]
        fn sysex_multi_packet() {
            let mut decoder = UsbMidiDecoder::<64>::new();
            assert!(decoder.feed([0x04, 0xF0, 0x01, 0x02]).is_none());
            assert!(decoder.feed([0x04, 0x03, 0x04, 0x05]).is_none());
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
            let result = decoder.feed([0x09, 0x90, 60, 100]);
            assert!(matches!(result, Some(MidiMessage::NoteOn { .. })));
        }
    }

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
            for i in 0..sink.len() {
                if let Some(packet) = sink.buf[i] {
                    if let Some(MidiMessage::Sysex(decoded)) = decoder.feed(packet) {
                        assert_eq!(decoded, &data);
                        return;
                    }
                }
            }
            panic!("no sysex message produced");
        }
    }
}
