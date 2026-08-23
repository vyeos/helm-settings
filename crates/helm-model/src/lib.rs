//! Canonical, compositor-neutral settings model.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Stable identifier shared by the GUI, CLI, history, and plugins.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SettingId(pub String);
