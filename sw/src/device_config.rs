#[derive(Debug, Clone, Copy)]
pub struct SwitchConfig {
    pub released_value: u8,
    pub pressed_value: u8,
}

impl Default for SwitchConfig {
    fn default() -> Self {
        Self {
            released_value: 0,
            pressed_value: 127,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ContinuousConfig {
    pub minimum_input: u8,
    pub maximum_input: u8,
    pub minimum_output: u8,
    pub maximum_output: u8,
    pub drive: u8,
}

impl Default for ContinuousConfig {
    fn default() -> Self {
        Self {
            minimum_input: 0,
            maximum_input: 127,
            minimum_output: 0,
            maximum_output: 127,
            drive: 64,
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, strum::Display, strum::VariantArray)]
pub enum InputMode {
    #[default]
    Continuous,
    Switch,
    #[strum(to_string = "Momentary as Toggle")]
    MomentaryAsToggle,
    #[strum(to_string = "Toggle as Momentary")]
    ToggleAsMomentary,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct InputConfig {
    pub mode: InputMode,
    pub switch: SwitchConfig,
    pub continuous: ContinuousConfig,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct ChannelConfig {
    pub input: InputConfig,
    pub cc: u8,
    pub label: [u8; ChannelConfig::LABEL_SIZE],
}

impl ChannelConfig {
    const LABEL_SIZE: usize = 32;

    pub fn from_index(index: usize) -> Self {
        Self::default().with_cc(index as u8)
    }

    pub fn with_input_mode(mut self, mode: InputMode) -> Self {
        self.input.mode = mode;
        self
    }

    pub fn with_released_value(mut self, value: u8) -> Self {
        self.input.switch.released_value = value;
        self
    }

    pub fn with_pressed_value(mut self, value: u8) -> Self {
        self.input.switch.pressed_value = value;
        self
    }

    pub fn with_minimum_input(mut self, value: u8) -> Self {
        self.input.continuous.minimum_input = value;
        self
    }

    pub fn with_maximum_input(mut self, value: u8) -> Self {
        self.input.continuous.maximum_input = value;
        self
    }

    pub fn with_minimum_output(mut self, value: u8) -> Self {
        self.input.continuous.minimum_output = value;
        self
    }

    pub fn with_maximum_output(mut self, value: u8) -> Self {
        self.input.continuous.maximum_output = value;
        self
    }

    pub fn with_drive(mut self, value: u8) -> Self {
        self.input.continuous.drive = value;
        self
    }

    pub fn with_cc(mut self, value: u8) -> Self {
        self.cc = value;
        self
    }

    pub fn with_label(mut self, label: [u8; Self::LABEL_SIZE]) -> Self {
        self.label = label;
        self
    }

    pub fn with_label_str(self, label_str: &str) -> Self {
        self.with_label(std::array::from_fn(|i| {
            label_str.as_bytes().get(i).copied().unwrap_or(0)
        }))
    }

    pub fn label_str(&self) -> &str {
        // Find the first null byte or use the full length
        let end = self
            .label
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(Self::LABEL_SIZE);
        std::str::from_utf8(&self.label[..end]).unwrap_or("")
    }
}

#[derive(Debug)]
pub struct DeviceConfig<const C: usize> {
    pub channels: [ChannelConfig; C],
}

impl<const C: usize> Default for DeviceConfig<C> {
    fn default() -> Self {
        Self {
            channels: std::array::from_fn(ChannelConfig::from_index),
        }
    }
}
