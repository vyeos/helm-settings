//! Canonical, compositor-neutral settings model.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Stable identifier shared by the GUI, CLI, history, and plugins.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SettingId(pub String);

impl SettingId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SettingValue {
    Bool(bool),
    Integer(i64),
    Decimal(f64),
    Text(String),
    Choice(String),
    Color(String),
    Path(String),
    List(Vec<SettingValue>),
    Object(BTreeMap<String, SettingValue>),
    Unset,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    Bool,
    Integer,
    Decimal,
    Text,
    Choice,
    Color,
    Path,
    Keybinding,
    DisplayLayout,
    Object,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Routine,
    RestartRequired,
    SessionImpact,
    DisplayCritical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyBehavior {
    Reload,
    NextLaunch,
    NextSession,
    ConfirmWithRollback,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Constraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

impl Constraints {
    #[must_use]
    pub const fn unconstrained() -> Self {
        Self {
            minimum: None,
            maximum: None,
            choices: Vec::new(),
            pattern: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SettingDefinition {
    pub id: SettingId,
    pub module: String,
    pub label: String,
    pub description: String,
    pub kind: SettingKind,
    pub risk: Risk,
    pub apply: ApplyBehavior,
    pub constraints: Constraints,
    pub sensitive: bool,
    pub writable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceOwnership {
    Helm,
    User,
    System,
    Runtime,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SettingState {
    pub definition: SettingDefinition,
    pub effective: SettingValue,
    pub source: SourceOwnership,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hint: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Missing,
    UnsupportedVersion,
    ReadOnly,
    Unhealthy,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub id: String,
    pub display_name: String,
    pub availability: Availability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentReport {
    pub schema_version: u32,
    pub session: String,
    pub components: Vec<ComponentStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_values_have_stable_tagged_json() {
        let json =
            serde_json::to_value(SettingValue::Choice("dwindle".into())).expect("serializes");
        assert_eq!(json, serde_json::json!({"type":"choice","value":"dwindle"}));
    }

    #[test]
    fn unconstrained_omits_empty_fields() {
        let json = serde_json::to_value(Constraints::unconstrained()).expect("serializes");
        assert_eq!(json, serde_json::json!({}));
    }
}
