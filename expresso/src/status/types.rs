use serde::{Deserialize, Serialize};

/// Events emitted by the various subsystems to report their current activity.
///
/// Persistent events (`Power`, `UsbConnected`) represent ongoing states.
/// All other events are one-shot triggers that produce a timed animation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum StatusEvent {
    /// Device is powered on (`true`) or off (`false`).
    Power(bool),
    /// USB host connected (`true`) or disconnected (`false`).
    UsbConnected(bool),
    /// An incoming USB MIDI message was received.
    MidiUsbIn,
    /// An outgoing USB MIDI message was sent.
    MidiUsbOut,
    /// An incoming DIN MIDI message was received.
    MidiDinIn,
    /// An outgoing DIN MIDI message was sent.
    MidiDinOut,
    /// An expression pedal generated a MIDI message.
    MidiExpression,
    /// Settings were updated via SysEx.
    SettingsUpdate,
}
