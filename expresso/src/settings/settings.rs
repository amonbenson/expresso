use serde::{Deserialize, Serialize};

use super::ExpressionGroupSettings;

#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub expression: ExpressionGroupSettings,
}
