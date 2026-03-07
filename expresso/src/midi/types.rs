#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MidiMessage {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    ControlChange { channel: u8, control: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    PitchBend { channel: u8, value: i16 },
}

#[derive(Clone, Copy, Debug)]
pub enum MidiEndpoint {
    Usb,
    Din,
    Expression,
}

// Returned by decoders. `Sysex` borrows from the decoder's internal buffer
// and is valid only until the next call to `feed`.
pub enum DecodeResult<'a> {
    Message(MidiMessage),
    Sysex(&'a [u8]),
}
