use core::cell::RefCell;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::pubsub::{PubSubChannel, Subscriber};
use expresso::midi::{MidiEndpoint, MidiMessage};
use expresso::settings::Settings;

pub use expresso::status::StatusEvent;

pub const MSG_CAP: usize = 16;

// Messages routed INTO the router carry their source endpoint as a tag.
pub type InMsg = (MidiMessage, MidiEndpoint);
pub type InMsgChannel = Channel<CriticalSectionRawMutex, InMsg, MSG_CAP>;
pub type InMsgSender = Sender<'static, CriticalSectionRawMutex, InMsg, MSG_CAP>;
pub type InMsgReceiver = Receiver<'static, CriticalSectionRawMutex, InMsg, MSG_CAP>;

// Shared settings protected by a blocking critical-section mutex.
pub type SettingsMutex = Mutex<CriticalSectionRawMutex, RefCell<Settings>>;

// Messages routed OUT of the router carry only the payload.
pub type MsgChannel = Channel<CriticalSectionRawMutex, MidiMessage, MSG_CAP>;
pub type MsgSender = Sender<'static, CriticalSectionRawMutex, MidiMessage, MSG_CAP>;
pub type MsgReceiver = Receiver<'static, CriticalSectionRawMutex, MidiMessage, MSG_CAP>;

// Status events. Published by all subsystems; subscribed by status_led and usb_midi.
//
// CAP = per-subscriber queue depth
// SUBS = max subscribers (status_led + usb_midi)
// PUBS = 0 — all publishers use dyn_publisher() which doesn't take a slot
pub const STATUS_CAP: usize = 8;
pub const STATUS_SUBS: usize = 2;
pub type StatusChannel =
    PubSubChannel<CriticalSectionRawMutex, StatusEvent, STATUS_CAP, STATUS_SUBS, 1>;
pub type StatusSubscriber =
    Subscriber<'static, CriticalSectionRawMutex, StatusEvent, STATUS_CAP, STATUS_SUBS, 1>;
