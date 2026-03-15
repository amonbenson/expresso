#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MidiMessage {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    ControlChange { channel: u8, control: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    PitchBend { channel: u8, value: i16 },
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MidiEndpoint {
    Usb,
    Din,
    Expression,
}

pub enum DecodeResult<'a> {
    Message(MidiMessage),
    Sysex(&'a [u8]),
}
