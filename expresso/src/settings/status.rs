use serde::{Deserialize, Serialize};

/// An abstract three-channel color value.
///
/// The semantics of the channels are left to the consumer (e.g. an RGB LED,
/// a UI indicator, a log entry color).  A value of `BLACK` (all zeros) means
/// the corresponding event is disabled and will be ignored entirely.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const BLACK: Self = Self::new(0, 0, 0);

    pub fn is_black(self) -> bool {
        self.r == 0 && self.g == 0 && self.b == 0
    }

    pub fn blend(self, other: Self) -> Self {
        Self {
            r: self.r.max(other.r),
            g: self.g.max(other.g),
            b: self.b.max(other.b),
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

/// Colors assigned to each status event.
///
/// Setting a color to `Color::BLACK` disables the corresponding event
/// entirely — no animation will be queued and no background value will be
/// contributed to the output.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StatusSettings {
    /// Persistent background while the device is powered.
    pub power: Color,
    /// Persistent background while a USB host is connected.
    pub usb_connected: Color,
    /// Flash color for incoming USB MIDI messages.
    pub midi_usb_in: Color,
    /// Flash color for outgoing USB MIDI messages.
    pub midi_usb_out: Color,
    /// Flash color for incoming DIN MIDI messages.
    pub midi_din_in: Color,
    /// Flash color for outgoing DIN MIDI messages.
    pub midi_din_out: Color,
    /// Flash color for expression pedal MIDI messages.
    pub midi_exp: Color,
    /// Flash color when settings are written via SysEx.
    pub settings_update: Color,
}

impl Default for StatusSettings {
    fn default() -> Self {
        Self {
            power: Color::new(255, 0, 0),
            usb_connected: Color::new(0, 255, 0),
            midi_usb_in: Color::new(0, 0, 255),
            midi_usb_out: Color::new(0, 0, 255),
            midi_din_in: Color::new(0, 0, 255),
            midi_din_out: Color::new(0, 0, 255),
            midi_exp: Color::new(0, 0, 255),
            settings_update: Color::new(0, 0, 255),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StatusSettingsPatch {
    Power(Color),
    UsbConnected(Color),
    MidiUsbIn(Color),
    MidiUsbOut(Color),
    MidiDinIn(Color),
    MidiDinOut(Color),
    MidiExp(Color),
    SettingsUpdate(Color),
}

impl StatusSettings {
    pub fn apply_patch(&mut self, patch: StatusSettingsPatch) {
        match patch {
            StatusSettingsPatch::Power(c) => self.power = c,
            StatusSettingsPatch::UsbConnected(c) => self.usb_connected = c,
            StatusSettingsPatch::MidiUsbIn(c) => self.midi_usb_in = c,
            StatusSettingsPatch::MidiUsbOut(c) => self.midi_usb_out = c,
            StatusSettingsPatch::MidiDinIn(c) => self.midi_din_in = c,
            StatusSettingsPatch::MidiDinOut(c) => self.midi_din_out = c,
            StatusSettingsPatch::MidiExp(c) => self.midi_exp = c,
            StatusSettingsPatch::SettingsUpdate(c) => self.settings_update = c,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_black_constant_is_zero() {
        assert_eq!(Color::BLACK, Color::new(0, 0, 0));
        assert!(Color::BLACK.is_black());
    }

    #[test]
    fn color_default_is_black() {
        assert_eq!(Color::default(), Color::BLACK);
    }

    // One test covers all eight patch variants by iterating over them.
    #[test]
    fn all_patch_variants_update_correct_field() {
        let c = Color::new(1, 2, 3);
        let cases: &[(StatusSettingsPatch, fn(&StatusSettings) -> Color)] = &[
            (StatusSettingsPatch::Power(c), |s| s.power),
            (StatusSettingsPatch::UsbConnected(c), |s| s.usb_connected),
            (StatusSettingsPatch::MidiUsbIn(c), |s| s.midi_usb_in),
            (StatusSettingsPatch::MidiUsbOut(c), |s| s.midi_usb_out),
            (StatusSettingsPatch::MidiDinIn(c), |s| s.midi_din_in),
            (StatusSettingsPatch::MidiDinOut(c), |s| s.midi_din_out),
            (StatusSettingsPatch::MidiExp(c), |s| s.midi_exp),
            (StatusSettingsPatch::SettingsUpdate(c), |s| {
                s.settings_update
            }),
        ];
        for (patch, get) in cases {
            let mut s = StatusSettings::default();
            s.apply_patch(*patch);
            assert_eq!(get(&s), c, "patch {patch:?} did not update the right field");
        }
    }

    #[test]
    fn patch_only_changes_target_field() {
        let mut s = StatusSettings::default();
        let original = s;
        s.apply_patch(StatusSettingsPatch::MidiDinOut(Color::new(9, 9, 9)));
        assert_eq!(s.midi_din_out, Color::new(9, 9, 9));
        assert_eq!(s.power, original.power);
        assert_eq!(s.midi_usb_in, original.midi_usb_in);
    }
}
