use serde::{Deserialize, Serialize};

use super::{ExpressionChannelSettings, ExpressionGroupSettings};

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Settings {
    pub expression: ExpressionGroupSettings,
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
