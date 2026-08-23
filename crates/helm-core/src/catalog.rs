use helm_model::{ApplyBehavior, Constraints, Risk, SettingDefinition, SettingId, SettingKind};

#[must_use]
pub fn foundation_catalog() -> Vec<SettingDefinition> {
    let mut catalog = desktop_catalog();
    catalog.extend(product_catalog());
    catalog
}

fn desktop_catalog() -> Vec<SettingDefinition> {
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

fn product_catalog() -> Vec<SettingDefinition> {
    vec![
        setting(
            "appearance.theme",
            "appearance",
            "Desktop theme",
            "Semantic palette shared by supported applications",
            SettingKind::Choice,
            Constraints::unconstrained(),
        ),
        setting(
            "appearance.wallpaper",
            "wallpaper",
            "Wallpaper",
            "Per-output image and fit mode through hyprpaper",
            SettingKind::Path,
            Constraints::unconstrained(),
        ),
        setting(
            "applications.alacritty.theme",
            "applications",
            "Alacritty theme",
            "Managed imported terminal palette",
            SettingKind::Choice,
            Constraints::unconstrained(),
        ),
        setting(
            "applications.yazi.flavor",
            "applications",
            "Yazi flavor",
            "Dark and light Yazi flavor selection",
            SettingKind::Choice,
            Constraints::unconstrained(),
        ),
        setting(
            "bars.layout",
            "applications",
            "Bar layout",
            "Managed Waybar or cooperative Quickshell layout",
            SettingKind::Object,
            Constraints::unconstrained(),
        ),
        setting(
            "desktop.startup",
            "profiles",
            "Startup applications",
            "User-scoped freedesktop autostart entries",
            SettingKind::Object,
            Constraints::unconstrained(),
        ),
        setting(
            "desktop.defaults",
            "profiles",
            "Default applications",
            "MIME handlers in the user mimeapps.list",
            SettingKind::Object,
            Constraints::unconstrained(),
        ),
        setting(
            "hyprland.window_rules",
            "profiles",
            "Window rules",
            "Named Lua rules matched by class or title",
            SettingKind::Object,
            Constraints::unconstrained(),
        ),
        setting(
            "profiles.active",
            "profiles",
            "Active profile",
            "Atomically apply a complete desired desktop state",
            SettingKind::Choice,
            Constraints::unconstrained(),
        ),
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
