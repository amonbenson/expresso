use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, Receiver, Sender},
};

pub const CHANNEL_CAP: usize = 16;

pub type MidiEventChannel = Channel<CriticalSectionRawMutex, MidiEvent, CHANNEL_CAP>;
pub type MidiSender<'a> = Sender<'a, CriticalSectionRawMutex, MidiEvent, CHANNEL_CAP>;
pub type MidiReceiver<'a> = Receiver<'a, CriticalSectionRawMutex, MidiEvent, CHANNEL_CAP>;

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

#[derive(Clone, Copy, Debug, defmt::Format)]
pub enum MidiPeripheral {
    Usb,
    Din,
    Expression(u8),
}

#[derive(Clone, Copy, Debug, defmt::Format)]
pub struct MidiEvent {
    pub source: MidiPeripheral,
    pub message: MidiMessage,
}

impl MidiEvent {
    pub fn new(source: MidiPeripheral, message: MidiMessage) -> Self {
        Self { source, message }
    }
}

/// A bidirectional MIDI peripheral (e.g. USB MIDI, DIN MIDI).
/// Receives events from the router and forwards events to the shared bus.
pub trait MidiBridge {
    async fn run(self, from_router: MidiReceiver<'static>, to_bus: MidiSender<'static>);
}

/// A producer-only MIDI peripheral (e.g. expression pedal inputs).
/// Only emits events onto the shared bus; never receives from the router.
pub trait MidiSource {
    async fn run(self, to_bus: MidiSender<'static>);
}
