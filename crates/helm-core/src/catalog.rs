use helm_model::{ApplyBehavior, Constraints, Risk, SettingDefinition, SettingId, SettingKind};

#[must_use]
pub fn foundation_catalog() -> Vec<SettingDefinition> {
    vec![
        setting(
            "hyprland.general.gaps_in",
            "desktop",
            "Inner gaps",
            "Space between tiled windows",
            SettingKind::Integer,
            Constraints {
                minimum: Some(0.0),
                maximum: Some(100.0),
                choices: Vec::new(),
                pattern: None,
            },
        ),
        setting(
            "hyprland.general.border_size",
            "desktop",
            "Border size",
            "Width of the active window border",
            SettingKind::Integer,
            Constraints {
                minimum: Some(0.0),
                maximum: Some(20.0),
                choices: Vec::new(),
                pattern: None,
            },
        ),
        setting(
            "hyprland.input.follow_mouse",
            "desktop",
            "Focus follows pointer",
            "Move keyboard focus as the pointer enters a window",
            SettingKind::Bool,
            Constraints::unconstrained(),
        ),
        SettingDefinition {
            id: SettingId::new("hyprland.displays.layout"),
            module: "displays".into(),
            label: "Display layout".into(),
            description: "Mode, position, scale and transform for connected outputs".into(),
            kind: SettingKind::DisplayLayout,
            risk: Risk::DisplayCritical,
            apply: ApplyBehavior::ConfirmWithRollback,
            constraints: Constraints::unconstrained(),
            sensitive: false,
            writable: true,
        },
    ]
}

fn setting(
    id: &str,
    module: &str,
    label: &str,
    description: &str,
    kind: SettingKind,
    constraints: Constraints,
) -> SettingDefinition {
    SettingDefinition {
        id: SettingId::new(id),
        module: module.into(),
        label: label.into(),
        description: description.into(),
        kind,
        risk: Risk::Routine,
        apply: ApplyBehavior::Reload,
        constraints,
        sensitive: false,
        writable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    #[test]
    fn catalog_ids_are_unique() {
        let catalog = foundation_catalog();
        let unique = catalog
            .iter()
            .map(|setting| &setting.id)
            .collect::<HashSet<_>>();
        assert_eq!(catalog.len(), unique.len());
    }
}
