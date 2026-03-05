use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Receiver, Sender},
};

pub const CHANNEL_CAP: usize = 16;

pub type MidiMessageChannel = Channel<CriticalSectionRawMutex, MidiMessage, CHANNEL_CAP>;
pub type MidiMessageSender<'a> = Sender<'a, CriticalSectionRawMutex, MidiMessage, CHANNEL_CAP>;
pub type MidiMessageReceiver<'a> = Receiver<'a, CriticalSectionRawMutex, MidiMessage, CHANNEL_CAP>;

#[derive(Clone, Copy, Debug, defmt::Format)]
pub enum MidiMessage {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    ControlChange { channel: u8, control: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    PitchBend { channel: u8, value: i16 },
    ActiveSensing,
    TimingClock,
}
