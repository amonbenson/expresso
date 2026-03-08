use serde::{Deserialize, Serialize};

use crate::config::NUM_CHANNELS;

use super::ExpressionChannelSettings;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExpressionGroupSettings {
    pub channels: [ExpressionChannelSettings; NUM_CHANNELS],
}

impl Default for ExpressionGroupSettings {
    fn default() -> Self {
        Self {
            channels: core::array::from_fn(ExpressionChannelSettings::new),
        }
    }
}
