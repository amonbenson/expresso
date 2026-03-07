use serde::{Deserialize, Serialize};

pub trait Adjustable {
    type Settings;

    fn update_settings(&mut self, settings: &Self::Settings);
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InputMode {
    Continuous,
    Switch,
    #[default]
    Compat,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct InputSettings {
    pub mode: InputMode,
    pub continuous: ContinuousSettings,
    pub switch: SwitchSettings,
}

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ExpressionChannelSettings {
    pub input: InputSettings,
    pub cc: u8,
    pub label: [u8; ExpressionChannelSettings::LABEL_SIZE],
}

impl ExpressionChannelSettings {
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

// Serde's derive can't satisfy `[ExpressionChannelSettings; C]: Serialize` for a const-generic
// `C` in all serde versions, so Serialize/Deserialize are implemented manually using tuple
// format (no length prefix), which matches postcard's wire format for fixed-size arrays.
#[derive(Debug, Clone, Copy)]
pub struct ExpressionGroupSettings<const C: usize> {
    pub channels: [ExpressionChannelSettings; C],
}

impl<const C: usize> Default for ExpressionGroupSettings<C> {
    fn default() -> Self {
        Self {
            channels: core::array::from_fn(ExpressionChannelSettings::new),
        }
    }
}

impl<const C: usize> Serialize for ExpressionGroupSettings<C> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeTuple;
        let mut tup = serializer.serialize_tuple(C)?;
        for ch in &self.channels {
            tup.serialize_element(ch)?;
        }
        tup.end()
    }
}

impl<'de, const C: usize> Deserialize<'de> for ExpressionGroupSettings<C> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V<const C: usize>;

        impl<'de, const C: usize> serde::de::Visitor<'de> for V<C> {
            type Value = ExpressionGroupSettings<C>;

            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "a tuple of {C} ExpressionChannelSettings")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut channels =
                    core::array::from_fn(|_| ExpressionChannelSettings::default());
                for (i, slot) in channels.iter_mut().enumerate() {
                    *slot = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::invalid_length(i, &self))?;
                }
                Ok(ExpressionGroupSettings { channels })
            }
        }

        deserializer.deserialize_tuple(C, V::<C>)
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct Settings<const C: usize> {
    pub expression: ExpressionGroupSettings<C>,
}

impl<const C: usize> Serialize for Settings<C> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.expression.serialize(serializer)
    }
}

impl<'de, const C: usize> Deserialize<'de> for Settings<C> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Settings {
            expression: ExpressionGroupSettings::deserialize(deserializer)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_label() {
        let mut settings = ExpressionChannelSettings::default();

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
