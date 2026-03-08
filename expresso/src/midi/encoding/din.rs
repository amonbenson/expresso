use super::super::{DecodeResult, MidiDecoder, MidiEncoder, MidiMessage, PacketSink};

// ---- DIN MIDI Encoder ----

pub struct DinMidiEncoder;

impl DinMidiEncoder {
    /// Encode a channel message as raw DIN MIDI bytes and emit them to `sink`.
    pub fn emit_bytes<S>(&mut self, message: &MidiMessage, sink: &mut S) -> Result<(), S::Error>
    where
        S: PacketSink<Packet = u8>,
    {
        match message {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => {
                sink.emit(0x90 | (channel & 0x0F))?;
                sink.emit(*note)?;
                sink.emit(*velocity)?;
            }
            MidiMessage::NoteOff {
                channel,
                note,
                velocity,
            } => {
                sink.emit(0x80 | (channel & 0x0F))?;
                sink.emit(*note)?;
                sink.emit(*velocity)?;
            }
            MidiMessage::ControlChange {
                channel,
                control,
                value,
            } => {
                sink.emit(0xB0 | (channel & 0x0F))?;
                sink.emit(*control)?;
                sink.emit(*value)?;
            }
            MidiMessage::ProgramChange { channel, program } => {
                sink.emit(0xC0 | (channel & 0x0F))?;
                sink.emit(*program)?;
            }
            MidiMessage::PitchBend { channel, value } => {
                let u = (*value + 8192) as u16;
                sink.emit(0xE0 | (channel & 0x0F))?;
                sink.emit((u & 0x7F) as u8)?;
                sink.emit(((u >> 7) & 0x7F) as u8)?;
            }
        }
        Ok(())
    }

    /// Encode a raw SysEx payload (must include leading 0xF0 and trailing 0xF7)
    /// as raw DIN MIDI bytes and emit them to `sink`.
    pub fn emit_sysex_bytes<S>(&mut self, payload: &[u8], sink: &mut S) -> Result<(), S::Error>
    where
        S: PacketSink<Packet = u8>,
    {
        for &b in payload {
            sink.emit(b)?;
        }
        Ok(())
    }
}

impl MidiEncoder for DinMidiEncoder {
    type Packet = u8;

    fn emit<S>(&mut self, message: &MidiMessage, sink: &mut S) -> Result<(), S::Error>
    where
        S: PacketSink<Packet = u8>,
    {
        self.emit_bytes(message, sink)
    }
}

// ---- DIN MIDI Decoder ----

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

    fn try_complete(&mut self) -> Option<MidiMessage> {
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

    fn feed(&mut self, byte: u8) -> Option<DecodeResult<'_>> {
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
                return Some(DecodeResult::Sysex(&self.sysex_buf[..len]));
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
        self.try_complete().map(DecodeResult::Message)
    }

    fn reset(&mut self) {
        self.status = 0;
        self.count = 0;
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

        fn emit(&mut self, packet: T) -> Result<(), SinkFullError> {
            if self.len >= N {
                return Err(SinkFullError);
            }
            self.buf[self.len] = Some(packet);
            self.len += 1;
            Ok(())
        }
    }

    // Feed all bytes and return the result of the last one.
    fn feed_all<'d, const N: usize>(
        decoder: &'d mut DinMidiDecoder<N>,
        bytes: &[u8],
    ) -> Option<DecodeResult<'d>> {
        match bytes.split_last() {
            None => None,
            Some((last, rest)) => {
                for &b in rest {
                    decoder.feed(b);
                }
                decoder.feed(*last)
            }
        }
    }

    // Convenience: feed bytes and return the channel message, panicking on sysex.
    fn feed_message<const N: usize>(
        decoder: &mut DinMidiDecoder<N>,
        bytes: &[u8],
    ) -> Option<MidiMessage> {
        match feed_all(decoder, bytes)? {
            DecodeResult::Message(msg) => Some(msg),
            DecodeResult::Sysex(_) => panic!("expected channel message, got sysex"),
        }
    }

    mod din_encoder {
        use super::*;

        fn encode(message: &MidiMessage) -> CollectSink<u8, 16> {
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
            let mut encoder = DinMidiEncoder;
            let mut sink = CollectSink::<u8, 16>::new();
            encoder.emit_sysex_bytes(&data, &mut sink).unwrap();
            assert_eq!(sink.len(), 4);
            assert_eq!(sink.get(0), 0xF0);
            assert_eq!(sink.get(1), 0x41);
            assert_eq!(sink.get(2), 0x10);
            assert_eq!(sink.get(3), 0xF7);
        }
    }

    mod din_decoder {
        use super::*;

        #[test]
        fn note_on() {
            let mut decoder = DinMidiDecoder::<0>::new();
            assert!(decoder.feed(0x90).is_none());
            assert!(decoder.feed(60).is_none());
            let msg = match decoder.feed(100) {
                Some(DecodeResult::Message(m)) => m,
                _ => panic!("expected channel message"),
            };
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
            let msg = feed_message(&mut decoder, &[0x90, 60, 0]).unwrap();
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
            let msg = feed_message(&mut decoder, &[0x83, 48, 64]).unwrap();
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
            let msg = feed_message(&mut decoder, &[0xB2, 7, 127]).unwrap();
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
            let msg = feed_message(&mut decoder, &[0xC0, 42]).unwrap();
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
            let msg = feed_message(&mut decoder, &[0xE0, 0x00, 0x40]).unwrap();
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
            let msg = feed_message(&mut decoder, &[0xE0, 0x00, 0x00]).unwrap();
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
            let msg = feed_message(&mut decoder, &[0xE0, 0x7F, 0x7F]).unwrap();
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
            let mut decoder = DinMidiDecoder::<0>::new();
            let msg1 = feed_message(&mut decoder, &[0x90, 60, 100]).unwrap();
            assert!(matches!(
                msg1,
                MidiMessage::NoteOn {
                    note: 60,
                    velocity: 100,
                    ..
                }
            ));
            let msg2 = feed_message(&mut decoder, &[64, 80]).unwrap();
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
            assert!(decoder.feed(0x60).is_none());
            assert!(decoder.feed(0x40).is_none());
            let msg = feed_message(&mut decoder, &[0x90, 60, 100]).unwrap();
            assert!(matches!(msg, MidiMessage::NoteOn { .. }));
        }

        #[test]
        fn status_byte_resets_parser() {
            let mut decoder = DinMidiDecoder::<0>::new();
            assert!(decoder.feed(0x90).is_none());
            assert!(decoder.feed(60).is_none());
            assert!(decoder.feed(0x80).is_none());
            let msg = feed_message(&mut decoder, &[48, 64]).unwrap();
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
            match feed_all(&mut decoder, &data).unwrap() {
                DecodeResult::Sysex(decoded) => assert_eq!(decoded, &data),
                _ => panic!("expected sysex"),
            }
        }

        #[test]
        fn sysex_then_normal_message() {
            let mut decoder = DinMidiDecoder::<64>::new();
            feed_all(&mut decoder, &[0xF0, 0x01, 0x02, 0xF7]);
            let msg = feed_message(&mut decoder, &[0x90, 60, 100]).unwrap();
            assert!(matches!(msg, MidiMessage::NoteOn { .. }));
        }

        #[test]
        fn sysex_overflow_is_silent() {
            let mut decoder = DinMidiDecoder::<4>::new();
            feed_all(&mut decoder, &[0xF0, 0x01, 0x02, 0x03, 0x04, 0x05, 0xF7]);
        }

        #[test]
        fn reset_clears_state() {
            let mut decoder = DinMidiDecoder::<0>::new();
            assert!(decoder.feed(0x90).is_none());
            assert!(decoder.feed(60).is_none());
            decoder.reset();
            assert!(decoder.feed(100).is_none());
        }

        #[test]
        fn multiple_messages_sequential() {
            let mut decoder = DinMidiDecoder::<0>::new();
            let msg1 = feed_message(&mut decoder, &[0x90, 60, 100]).unwrap();
            assert!(matches!(msg1, MidiMessage::NoteOn { note: 60, .. }));
            let msg2 = feed_message(&mut decoder, &[0x80, 60, 0]).unwrap();
            assert!(matches!(msg2, MidiMessage::NoteOff { note: 60, .. }));
            let msg3 = feed_message(&mut decoder, &[0xB0, 7, 64]).unwrap();
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

    mod din_roundtrip {
        use super::*;

        fn roundtrip_non_sysex(message: &MidiMessage) -> Option<MidiMessage> {
            let mut encoder = DinMidiEncoder;
            let mut sink = CollectSink::<u8, 16>::new();
            encoder.emit_bytes(message, &mut sink).unwrap();

            let mut decoder = DinMidiDecoder::<0>::new();
            let mut result = None;
            for i in 0..sink.len() {
                if let Some(byte) = sink.buf[i] {
                    if let Some(DecodeResult::Message(msg)) = decoder.feed(byte) {
                        result = Some(msg);
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
            assert!(matches!(
                roundtrip_non_sysex(&msg).unwrap(),
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
            assert!(matches!(
                roundtrip_non_sysex(&msg).unwrap(),
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
            assert!(matches!(
                roundtrip_non_sysex(&msg).unwrap(),
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
            assert!(matches!(
                roundtrip_non_sysex(&msg).unwrap(),
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
            let mut encoder = DinMidiEncoder;
            let mut sink = CollectSink::<u8, 16>::new();
            encoder.emit_sysex_bytes(&data, &mut sink).unwrap();

            let mut decoder = DinMidiDecoder::<64>::new();
            for i in 0..sink.len() {
                if let Some(byte) = sink.buf[i] {
                    if let Some(DecodeResult::Sysex(decoded)) = decoder.feed(byte) {
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
