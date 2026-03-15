use crate::midi::MidiEndpoint;
use crate::settings::{Color, StatusSettings};

use super::{MidiDirection, StatusEvent};

/// Number of update ticks a trigger animation stays active.
///
/// At the recommended update rate of 50 Hz this equals 100 ms.
pub const FLASH_TICKS: u32 = 5;

// Trigger slot indices — kept module-private.
const TRIG_MIDI_USB_IN: usize = 0;
const TRIG_MIDI_USB_OUT: usize = 1;
const TRIG_MIDI_DIN_IN: usize = 2;
const TRIG_MIDI_DIN_OUT: usize = 3;
const TRIG_MIDI_EXP: usize = 4;
const TRIG_SETTINGS: usize = 5;
const NUM_TRIGGERS: usize = 6;

/// A single-shot animation attached to a trigger slot.
///
/// Currently only `Flash` is implemented.  Future variants (double-flash,
/// pulse, breathe, …) can be added here without changing `StatusState`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Animation {
    /// Stay active for `remaining` ticks then expire.
    Flash { color: Color, remaining: u32 },
}

impl Animation {
    fn flash(color: Color) -> Self {
        Self::Flash {
            color,
            remaining: FLASH_TICKS,
        }
    }

    /// Advance by one tick.  Returns `true` while the animation is still active.
    fn tick(&mut self) -> bool {
        match self {
            Self::Flash { remaining, .. } => {
                if *remaining > 0 {
                    *remaining -= 1;
                }
                *remaining > 0
            }
        }
    }

    fn color(&self) -> Color {
        match self {
            Self::Flash { color, remaining } if *remaining > 0 => *color,
            _ => Color::BLACK,
        }
    }
}

/// Tracks the current status and computes a blended output color.
///
/// Call [`apply`] whenever a [`StatusEvent`] arrives (passing the current
/// [`StatusSettings`] so that black-colored events are discarded immediately),
/// then call [`tick`] on every update tick, and read [`color`] to obtain the
/// final blended output value.
///
/// The consumer (e.g. firmware) is responsible for mapping the returned
/// [`Color`] to a physical output such as PWM duty cycles.
pub struct StatusState {
    power: bool,
    usb_connected: bool,
    triggers: [Option<Animation>; NUM_TRIGGERS],
}

impl StatusState {
    pub fn new() -> Self {
        Self {
            power: false,
            usb_connected: false,
            triggers: [None; NUM_TRIGGERS],
        }
    }

    /// Process an incoming event.
    ///
    /// If the color assigned to the event in `settings` is `BLACK`, the event
    /// is ignored entirely — no animation is created and no state changes.
    pub fn apply(&mut self, event: StatusEvent, settings: &StatusSettings) {
        match event {
            StatusEvent::Power(on) => self.power = on,
            StatusEvent::UsbConnected(on) => self.usb_connected = on,
            StatusEvent::Midi {
                endpoint,
                direction,
                ..
            } => {
                let (slot, color) = match (endpoint, direction) {
                    (MidiEndpoint::Usb, MidiDirection::In) => {
                        (TRIG_MIDI_USB_IN, settings.midi_usb_in)
                    }
                    (MidiEndpoint::Usb, MidiDirection::Out) => {
                        (TRIG_MIDI_USB_OUT, settings.midi_usb_out)
                    }
                    (MidiEndpoint::Din, MidiDirection::In) => {
                        (TRIG_MIDI_DIN_IN, settings.midi_din_in)
                    }
                    (MidiEndpoint::Din, MidiDirection::Out) => {
                        (TRIG_MIDI_DIN_OUT, settings.midi_din_out)
                    }
                    (MidiEndpoint::Expression, _) => (TRIG_MIDI_EXP, settings.midi_exp),
                };
                self.arm(slot, color);
            }
            StatusEvent::SettingsUpdate => self.arm(TRIG_SETTINGS, settings.settings_update),
        }
    }

    fn arm(&mut self, slot: usize, color: Color) {
        if !color.is_black() {
            self.triggers[slot] = Some(Animation::flash(color));
        }
    }

    /// Advance all active animations by one tick, expiring those that finish.
    pub fn tick(&mut self) {
        for slot in &mut self.triggers {
            if let Some(anim) = slot {
                if !anim.tick() {
                    *slot = None;
                }
            }
        }
    }

    /// Compute the blended output color from all currently active sources.
    ///
    /// Persistent states are skipped when their assigned color is `BLACK`
    /// (avoids a pointless `max_mix` call).
    pub fn color(&self, settings: &StatusSettings) -> Color {
        let mut color = Color::BLACK;
        if self.power && !settings.power.is_black() {
            color = color.blend(settings.power);
        }
        if self.usb_connected && !settings.usb_connected.is_black() {
            color = color.blend(settings.usb_connected);
        }
        for slot in &self.triggers {
            if let Some(anim) = slot {
                color = color.blend(anim.color());
            }
        }
        color
    }
}

impl Default for StatusState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Color, StatusSettings};

    fn settings() -> StatusSettings {
        StatusSettings::default()
    }

    fn black_settings() -> StatusSettings {
        StatusSettings {
            power: Color::BLACK,
            usb_connected: Color::BLACK,
            midi_usb_in: Color::BLACK,
            midi_usb_out: Color::BLACK,
            midi_din_in: Color::BLACK,
            midi_din_out: Color::BLACK,
            midi_exp: Color::BLACK,
            settings_update: Color::BLACK,
        }
    }

    // ---- Color helpers ----

    #[test]
    fn color_is_black_true_for_black() {
        assert!(Color::BLACK.is_black());
        assert!(Color::new(0, 0, 0).is_black());
    }

    #[test]
    fn color_is_black_false_for_nonzero() {
        assert!(!Color::new(1, 0, 0).is_black());
        assert!(!Color::new(0, 1, 0).is_black());
        assert!(!Color::new(0, 0, 1).is_black());
    }

    #[test]
    fn color_max_mix_takes_per_channel_max() {
        let a = Color::new(100, 0, 50);
        let b = Color::new(0, 80, 60);
        assert_eq!(a.blend(b), Color::new(100, 80, 60));
    }

    #[test]
    fn color_max_mix_with_black_is_identity() {
        let c = Color::new(10, 20, 30);
        assert_eq!(c.blend(Color::BLACK), c);
        assert_eq!(Color::BLACK.blend(c), c);
    }

    // ---- Animation ----

    #[test]
    fn animation_flash_counts_down() {
        let mut a = Animation::flash(Color::new(0, 0, 120));
        let mut active_ticks = 0u32;
        for _ in 0..FLASH_TICKS + 5 {
            if a.tick() {
                active_ticks += 1;
            }
        }
        // Active for FLASH_TICKS - 1 ticks after the first tick call
        assert_eq!(active_ticks, FLASH_TICKS - 1);
    }

    #[test]
    fn animation_color_returns_black_when_expired() {
        let mut a = Animation::flash(Color::new(0, 0, 120));
        for _ in 0..FLASH_TICKS {
            a.tick();
        }
        assert_eq!(a.color(), Color::BLACK);
    }

    #[test]
    fn animation_color_returns_assigned_color_while_active() {
        let c = Color::new(0, 0, 120);
        let a = Animation::flash(c);
        assert_eq!(a.color(), c);
    }

    // ---- StatusState::apply — persistent events ----

    #[test]
    fn power_on_reflects_in_color() {
        let s = settings();
        let mut state = StatusState::new();
        state.apply(StatusEvent::Power(true), &s);
        assert_eq!(state.color(&s), s.power);
    }

    #[test]
    fn power_off_clears_background() {
        let s = settings();
        let mut state = StatusState::new();
        state.apply(StatusEvent::Power(true), &s);
        state.apply(StatusEvent::Power(false), &s);
        assert_eq!(state.color(&s), Color::BLACK);
    }

    #[test]
    fn usb_connected_adds_to_color() {
        let s = settings();
        let mut state = StatusState::new();
        state.apply(StatusEvent::Power(true), &s);
        state.apply(StatusEvent::UsbConnected(true), &s);
        let expected = s.power.blend(s.usb_connected);
        assert_eq!(state.color(&s), expected);
    }

    // ---- StatusState::apply — trigger events ----

    fn midi_cc() -> crate::midi::MidiMessage {
        crate::midi::MidiMessage::ControlChange {
            channel: 0,
            control: 0,
            value: 0,
        }
    }

    #[test]
    fn midi_usb_in_creates_flash() {
        let s = settings();
        let mut state = StatusState::new();
        state.apply(
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Usb,
                direction: MidiDirection::In,
                message: midi_cc(),
            },
            &s,
        );
        assert_eq!(state.color(&s), s.midi_usb_in);
    }

    #[test]
    fn all_trigger_events_are_handled() {
        let s = settings();
        let events = [
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Usb,
                direction: MidiDirection::In,
                message: midi_cc(),
            },
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Usb,
                direction: MidiDirection::Out,
                message: midi_cc(),
            },
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Din,
                direction: MidiDirection::In,
                message: midi_cc(),
            },
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Din,
                direction: MidiDirection::Out,
                message: midi_cc(),
            },
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Expression,
                direction: MidiDirection::Out,
                message: midi_cc(),
            },
            StatusEvent::SettingsUpdate,
        ];
        for event in events {
            let mut state = StatusState::new();
            state.apply(event, &s);
            assert_ne!(
                state.color(&s),
                Color::BLACK,
                "event {event:?} produced no color"
            );
        }
    }

    // ---- Black-color optimization ----

    #[test]
    fn black_color_trigger_is_not_armed() {
        let s = black_settings();
        let mut state = StatusState::new();
        state.apply(
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Usb,
                direction: MidiDirection::In,
                message: midi_cc(),
            },
            &s,
        );
        assert_eq!(state.color(&s), Color::BLACK);
    }

    #[test]
    fn black_color_power_is_not_shown() {
        let s = black_settings();
        let mut state = StatusState::new();
        state.apply(StatusEvent::Power(true), &s);
        assert_eq!(state.color(&s), Color::BLACK);
    }

    // ---- StatusState::tick ----

    #[test]
    fn flash_expires_after_flash_ticks() {
        let s = settings();
        let mut state = StatusState::new();
        state.apply(
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Usb,
                direction: MidiDirection::In,
                message: midi_cc(),
            },
            &s,
        );
        for _ in 0..FLASH_TICKS {
            state.tick();
        }
        assert_eq!(state.color(&s), Color::BLACK);
    }

    #[test]
    fn persistent_state_survives_ticks() {
        let s = settings();
        let mut state = StatusState::new();
        state.apply(StatusEvent::Power(true), &s);
        for _ in 0..100 {
            state.tick();
        }
        assert_eq!(state.color(&s), s.power);
    }

    #[test]
    fn re_arming_resets_flash_timer() {
        let s = settings();
        let mut state = StatusState::new();
        let ev = StatusEvent::Midi {
            endpoint: MidiEndpoint::Usb,
            direction: MidiDirection::In,
            message: midi_cc(),
        };
        state.apply(ev, &s);
        // Advance almost to expiry
        for _ in 0..FLASH_TICKS - 1 {
            state.tick();
        }
        // Re-arm
        state.apply(ev, &s);
        // Should still be active after the original would have expired
        for _ in 0..FLASH_TICKS - 1 {
            state.tick();
        }
        assert_ne!(
            state.color(&s),
            Color::BLACK,
            "flash expired too early after re-arm"
        );
    }

    // ---- Color blending ----

    #[test]
    fn color_blends_persistent_and_trigger() {
        let s = settings();
        let mut state = StatusState::new();
        state.apply(StatusEvent::Power(true), &s);
        state.apply(
            StatusEvent::Midi {
                endpoint: MidiEndpoint::Usb,
                direction: MidiDirection::In,
                message: midi_cc(),
            },
            &s,
        );
        let expected = s.power.blend(s.midi_usb_in);
        assert_eq!(state.color(&s), expected);
    }

    // ---- StatusSettings::apply_patch ----

    #[test]
    fn settings_patch_updates_color() {
        use crate::settings::StatusSettingsPatch;

        let mut s = StatusSettings::default();
        let new_color = Color::new(1, 2, 3);
        s.apply_patch(StatusSettingsPatch::Power(new_color));
        assert_eq!(s.power, new_color);
    }

    #[test]
    fn settings_patch_only_changes_target_field() {
        use crate::settings::StatusSettingsPatch;

        let original = StatusSettings::default();
        let mut s = original;
        s.apply_patch(StatusSettingsPatch::MidiUsbIn(Color::new(10, 20, 30)));
        assert_eq!(s.midi_usb_out, original.midi_usb_out);
        assert_eq!(s.power, original.power);
    }
}
