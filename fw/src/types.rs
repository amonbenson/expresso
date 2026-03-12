use core::cell::RefCell;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use expresso::midi::{MidiEndpoint, MidiMessage};
use expresso::settings::Settings;

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

// Status LED events. Persistent states use bool; triggers fire a one-time flash.
pub const STATUS_CAP: usize = 16;
pub type StatusChannel = Channel<CriticalSectionRawMutex, StatusEvent, STATUS_CAP>;
pub type StatusSender = Sender<'static, CriticalSectionRawMutex, StatusEvent, STATUS_CAP>;
pub type StatusReceiver = Receiver<'static, CriticalSectionRawMutex, StatusEvent, STATUS_CAP>;

#[derive(Clone, Copy)]
pub enum StatusEvent {
    // Persistent on/off states
    Power(bool),
    UsbConnected(bool),
    // One-time trigger events (produce a timed flash)
    MidiUsbIn,
    MidiUsbOut,
    MidiDinIn,
    MidiDinOut,
    MidiExpression,
    SettingsUpdate,
}
