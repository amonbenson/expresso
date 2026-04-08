use serde::{Deserialize, Serialize};

use crate::settings::SettingsPatch;

use super::{ExpressionGroupSettings, StatusSettings};

#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub expression: ExpressionGroupSettings,
    pub status: StatusSettings,
}

impl Settings {
    pub fn apply_patch(&mut self, patch: SettingsPatch) {
        match patch {
            SettingsPatch::ExpressionChannel(index, channel_patch) => {
                if let Some(channel) = self.expression.channels.get_mut(index) {
                    channel.apply_patch(channel_patch);
                }
            }
            SettingsPatch::Status(status_patch) => {
                self.status.apply_patch(status_patch);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::status::Color;
    use crate::settings::{ExpressionChannelSettingsPatch, StatusSettingsPatch};

    #[test]
    fn status_patch_routed_correctly() {
        let mut s = Settings::default();
        let new_color = Color::new(10, 20, 30);
        s.apply_patch(SettingsPatch::Status(StatusSettingsPatch::MidiExp(
            new_color,
        )));
        assert_eq!(s.status.midi_exp, new_color);
        // Other fields unchanged
        assert_eq!(s.status.power, StatusSettings::default().power);
    }

    #[test]
    fn expression_channel_patch_routed_correctly() {
        let mut s = Settings::default();
        s.apply_patch(SettingsPatch::ExpressionChannel(
            2,
            ExpressionChannelSettingsPatch::CC(77),
        ));
        assert_eq!(s.expression.channels[2].cc, 77);
        assert_eq!(
            s.expression.channels[0].cc,
            Settings::default().expression.channels[0].cc
        );
    }

    #[test]
    fn out_of_bounds_expression_channel_patch_is_ignored() {
        let mut s = Settings::default();
        let original = s;
        s.apply_patch(SettingsPatch::ExpressionChannel(
            99,
            ExpressionChannelSettingsPatch::CC(77),
        ));
        assert_eq!(s, original);
    }
}
