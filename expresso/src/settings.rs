pub trait Adjustable {
    type Settings;

    fn update_settings(&mut self, settings: &Self::Settings);
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    #[default]
    Continuous,
    Switch,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContinuousSettings {
    pub minimum_input: f32,
    pub maximum_input: f32,
    pub minimum_output: u8,
    pub maximum_output: u8,
    pub drive: f32,
}

impl Default for ContinuousSettings {
    fn default() -> Self {
        Self {
            minimum_input: 0.0,
            maximum_input: 1.0,
            minimum_output: 0,
            maximum_output: 127,
            drive: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchSettings {
    pub invert_polarity: bool,
    pub released_value: u8,
    pub pressed_value: u8,
}

impl Default for SwitchSettings {
    fn default() -> Self {
        Self {
            invert_polarity: false,
            released_value: 0,
            pressed_value: 127,
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub struct InputSettings {
    pub mode: InputMode,
    pub continuous: ContinuousSettings,
    pub switch: SwitchSettings,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct ChannelSettings {
    pub input: InputSettings,
    pub cc: u8,
    pub label: [u8; ChannelSettings::LABEL_SIZE],
}

impl ChannelSettings {
    const LABEL_SIZE: usize = 32;

    pub fn new(index: usize) -> Self {
        Self {
            cc: index as u8,
            ..Default::default()
        }
    }

    pub fn set_label_str(&mut self, label_str: &str) {
        self.label = core::array::from_fn(|i| label_str.as_bytes().get(i).copied().unwrap_or(0));
    }

    pub fn label_str(&self) -> &str {
        // Find the first null byte or use the full length
        let end = self
            .label
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(Self::LABEL_SIZE);
        core::str::from_utf8(&self.label[..end]).unwrap_or("")
    }
}

#[derive(Debug)]
pub struct DeviceSettings<const C: usize> {
    pub channels: [ChannelSettings; C],
}

impl<const C: usize> Default for DeviceSettings<C> {
    fn default() -> Self {
        Self {
            channels: core::array::from_fn(ChannelSettings::new),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_label() {
        let mut settings = ChannelSettings::default();

        // Check empty label
        settings.set_label_str("");
        assert_eq!(settings.label, [0; 32]);
        assert_eq!(settings.label_str(), "");

        // Check short name
        settings.set_label_str("Some Label!");
        assert_eq!(settings.label_str(), "Some Label!");

        // Check overflow condition
        settings.set_label_str("Some very large label that is definately too long");
        assert_eq!(settings.label_str(), "Some very large label that is de",);
    }
}
