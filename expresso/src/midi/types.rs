#[derive(Clone, Copy, Debug)]
pub enum MidiMessage<'a> {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    ControlChange { channel: u8, control: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    PitchBend { channel: u8, value: i16 },
    Sysex(&'a [u8]),
}

impl<'a> MidiMessage<'a> {
    pub fn to_static(self) -> Option<MidiMessage<'static>> {
        match self {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => Some(MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            }),
            MidiMessage::NoteOff {
                channel,
                note,
                velocity,
            } => Some(MidiMessage::NoteOff {
                channel,
                note,
                velocity,
            }),
            MidiMessage::ControlChange {
                channel,
                control,
                value,
            } => Some(MidiMessage::ControlChange {
                channel,
                control,
                value,
            }),
            MidiMessage::ProgramChange { channel, program } => {
                Some(MidiMessage::ProgramChange { channel, program })
            }
            MidiMessage::PitchBend { channel, value } => {
                Some(MidiMessage::PitchBend { channel, value })
            }
            MidiMessage::Sysex(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum MidiEndpoint {
    Usb,
    Din,
    Expression,
}
