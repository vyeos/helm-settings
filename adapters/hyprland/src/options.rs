use std::collections::{BTreeMap, BTreeSet};

use helm_model::{
    ApplyBehavior, Constraints, Risk, SettingDefinition, SettingId, SettingKind, SettingState,
    SettingValue, SourceOwnership,
};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct UpstreamDescription {
    pub name: String,
    pub description: String,
    pub default: serde_json::Value,
    pub current: serde_json::Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    #[serde(default)]
    pub map: Option<Vec<BTreeMap<String, i64>>>,
}

#[derive(Clone, Debug)]
pub struct CuratedOption {
    pub upstream_name: &'static str,
    pub stable_id: &'static str,
    pub label: &'static str,
    pub kind: SettingKind,
    pub risk: Risk,
}

#[derive(Clone, Debug, Default)]
pub struct OptionCatalog {
    pub settings: Vec<SettingState>,
    pub unknown: Vec<UpstreamDescription>,
}

impl OptionCatalog {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let descriptions: Vec<UpstreamDescription> = serde_json::from_str(json)?;
        let curated = curated_options();
        let names = curated
            .iter()
            .map(|item| item.upstream_name)
            .collect::<BTreeSet<_>>();
        let mut settings = Vec::new();
        let mut unknown = Vec::new();
        for description in descriptions {
            if let Some(item) = curated
                .iter()
                .find(|item| item.upstream_name == description.name)
            {
                settings.push(to_state(item, &description));
            } else if !names.contains(description.name.as_str()) {
                unknown.push(description);
            }
        }
        settings.sort_by(|left, right| left.definition.id.0.cmp(&right.definition.id.0));
        Ok(Self { settings, unknown })
    }
}

#[must_use]
pub fn curated_options() -> Vec<CuratedOption> {
    vec![
        option(
            "general:border_size",
            "hyprland.general.border_size",
            "Border size",
            SettingKind::Integer,
        ),
        option(
            "general:gaps_in",
            "hyprland.general.gaps_in",
            "Inner gaps",
            SettingKind::Integer,
        ),
        option(
            "general:gaps_out",
            "hyprland.general.gaps_out",
            "Outer gaps",
            SettingKind::Integer,
        ),
        option(
            "general:layout",
            "hyprland.general.layout",
            "Layout",
            SettingKind::Choice,
        ),
        option(
            "decoration:rounding",
            "hyprland.decoration.rounding",
            "Corner radius",
            SettingKind::Integer,
        ),
        option(
            "decoration:blur:enabled",
            "hyprland.decoration.blur.enabled",
            "Window blur",
            SettingKind::Bool,
        ),
        option(
            "animations:enabled",
            "hyprland.animations.enabled",
            "Animations",
            SettingKind::Bool,
        ),
        option(
            "input:follow_mouse",
            "hyprland.input.follow_mouse",
            "Focus follows pointer",
            SettingKind::Integer,
        ),
        option(
            "input:sensitivity",
            "hyprland.input.sensitivity",
            "Pointer sensitivity",
            SettingKind::Decimal,
        ),
        option(
            "misc:disable_hyprland_logo",
            "hyprland.misc.disable_logo",
            "Hide Hyprland logo",
            SettingKind::Bool,
        ),
    ]
}

const fn option(
    upstream_name: &'static str,
    stable_id: &'static str,
    label: &'static str,
    kind: SettingKind,
) -> CuratedOption {
    CuratedOption {
        upstream_name,
        stable_id,
        label,
        kind,
        risk: Risk::Routine,
    }
}

fn to_state(item: &CuratedOption, source: &UpstreamDescription) -> SettingState {
    let choices = source
        .map
        .as_ref()
        .map(|values| {
            values
                .iter()
                .flat_map(|entry| entry.keys().cloned())
                .collect()
        })
        .unwrap_or_default();
    let effective = json_value(&source.current, &item.kind);
    SettingState {
        definition: SettingDefinition {
            id: SettingId::new(item.stable_id),
            module: "desktop".into(),
            label: item.label.into(),
            description: source.description.clone(),
            kind: item.kind.clone(),
            risk: item.risk,
            apply: ApplyBehavior::Reload,
            constraints: Constraints {
                minimum: source.min,
                maximum: source.max,
                choices,
                pattern: None,
            },
            sensitive: false,
            writable: true,
        },
        effective,
        source: SourceOwnership::Runtime,
        source_hint: Some(source.name.clone()),
        warnings: Vec::new(),
    }
}

fn json_value(value: &serde_json::Value, kind: &SettingKind) -> SettingValue {
    match kind {
        SettingKind::Bool => value
            .as_bool()
            .map_or(SettingValue::Unset, SettingValue::Bool),
        SettingKind::Integer => value
            .as_i64()
            .map_or(SettingValue::Unset, SettingValue::Integer),
        SettingKind::Decimal => value
            .as_f64()
            .map_or(SettingValue::Unset, SettingValue::Decimal),
        SettingKind::Choice => value.as_str().map_or(SettingValue::Unset, |text| {
            SettingValue::Choice(text.into())
        }),
        _ => value
            .as_str()
            .map_or(SettingValue::Unset, |text| SettingValue::Text(text.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exposes_only_curated_options_as_writable() {
        let json = r#"[
          {"name":"general:border_size","description":"Border","default":1,"current":2,"min":0,"max":20,"map":null},
          {"name":"plugin:unknown","description":"Unknown","default":0,"current":1,"min":null,"max":null,"map":null}
        ]"#;
        let catalog = OptionCatalog::from_json(json).expect("catalog");
        assert_eq!(catalog.settings.len(), 1);
        assert_eq!(catalog.unknown.len(), 1);
        assert_eq!(catalog.settings[0].effective, SettingValue::Integer(2));
    }
}
