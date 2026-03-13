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
