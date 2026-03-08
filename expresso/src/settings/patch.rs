use serde::{Deserialize, Serialize};

use crate::settings::{ExpressionChannelSettings, InputMode};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ExpressionChannelSettingsPatch {
    Label([u8; ExpressionChannelSettings::LABEL_SIZE]),
    CC(u8),

    InputMode(InputMode),

    ContinuousMinimumInput(f32),
    ContinuousMaximumInput(f32),
    ContinuousMinimumOutput(u8),
    ContinuousMaximumOutput(u8),
    ContinuousDrive(f32),

    SwitchInvertPolarity(bool),
    SwitchReleasedValue(u8),
    SwitchPressedValue(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SettingsPatch {
    ExpressionChannel(usize, ExpressionChannelSettingsPatch),
}
