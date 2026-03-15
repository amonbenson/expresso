use serde::{Deserialize, Serialize};

use crate::midi::{MidiEndpoint, MidiMessage};

/// Direction of a MIDI message relative to this device.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum MidiDirection {
    In,
    Out,
}

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
    /// A MIDI message was sent or received on the given endpoint.
    Midi {
        endpoint: MidiEndpoint,
        direction: MidiDirection,
        message: MidiMessage,
    },
    /// Settings were updated via SysEx.
    SettingsUpdate,
}
