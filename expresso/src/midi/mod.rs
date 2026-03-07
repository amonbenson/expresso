// use embassy_sync::{
//     blocking_mutex::raw::CriticalSectionRawMutex,
//     channel::{Channel, Receiver, Sender},
// };

// pub const CHANNEL_CAP: usize = 16;

// pub type MidiMessageChannel = Channel<CriticalSectionRawMutex, MidiMessage, CHANNEL_CAP>;
// pub type MidiMessageSender<'a> = Sender<'a, CriticalSectionRawMutex, MidiMessage, CHANNEL_CAP>;
// pub type MidiMessageReceiver<'a> = Receiver<'a, CriticalSectionRawMutex, MidiMessage, CHANNEL_CAP>;

pub mod types;
pub use types::{DecodeResult, MidiMessage};

pub mod traits;
pub use traits::{MidiDecoder, MidiEncoder, MidiMessageSink, PacketSink};

pub mod encoding;
pub use encoding::*;
