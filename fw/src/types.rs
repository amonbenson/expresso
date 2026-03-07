use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use expresso::midi::MidiMessage;
use expresso::midi::types::MidiEndpoint;

pub const MSG_CAP: usize = 16;

// Messages routed INTO the router carry their source endpoint as a tag.
pub type InMsg = (MidiMessage<'static>, MidiEndpoint);
pub type InMsgChannel = Channel<CriticalSectionRawMutex, InMsg, MSG_CAP>;
pub type InMsgSender = Sender<'static, CriticalSectionRawMutex, InMsg, MSG_CAP>;
pub type InMsgReceiver = Receiver<'static, CriticalSectionRawMutex, InMsg, MSG_CAP>;

// Messages routed OUT of the router carry only the payload.
pub type MsgChannel = Channel<CriticalSectionRawMutex, MidiMessage<'static>, MSG_CAP>;
pub type MsgSender = Sender<'static, CriticalSectionRawMutex, MidiMessage<'static>, MSG_CAP>;
pub type MsgReceiver = Receiver<'static, CriticalSectionRawMutex, MidiMessage<'static>, MSG_CAP>;
