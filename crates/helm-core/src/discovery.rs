use std::{env, path::Path, process::Command};

use helm_model::{Availability, ComponentStatus, EnvironmentReport};

use crate::Error;

pub trait Probe {
    fn environment(&self, name: &str) -> Option<String>;
    fn executable(&self, name: &str) -> bool;
    fn output(&self, program: &str, arguments: &[&str]) -> Result<String, Error>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProbe;

impl Probe for SystemProbe {
    fn environment(&self, name: &str) -> Option<String> {
        env::var(name).ok()
    }

    fn executable(&self, name: &str) -> bool {
        env::var_os("PATH").is_some_and(|paths| {
            env::split_paths(&paths).any(|directory| {
                let path = directory.join(name);
                path.is_file() && is_executable(&path)
            })
        })
    }

    fn output(&self, program: &str, arguments: &[&str]) -> Result<String, Error> {
        let result = Command::new(program)
            .args(arguments)
            .output()
            .map_err(|error| Error::Probe(error.to_string()))?;
        if !result.status.success() {
            return Err(Error::Probe(format!(
                "{program} exited with {}",
                result.status.code().unwrap_or(-1)
            )));
        }
        String::from_utf8(result.stdout).map_err(|error| Error::Probe(error.to_string()))
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

pub struct DiscoveryService<P> {
    probe: P,
}

impl<P: Probe> DiscoveryService<P> {
    pub const fn new(probe: P) -> Self {
        Self { probe }
    }

    pub fn discover(&self) -> EnvironmentReport {
        let session = self
            .probe
            .environment("XDG_CURRENT_DESKTOP")
            .or_else(|| self.probe.environment("XDG_SESSION_DESKTOP"))
            .unwrap_or_else(|| "unknown".into());
        EnvironmentReport {
            schema_version: 1,
            session,
            components: vec![
                self.hyprland(),
                self.versioned("waybar", "Waybar", &["--version"], "0.15"),
                self.versioned("quickshell", "Quickshell", &["--version"], "0.3"),
                self.versioned("alacritty", "Alacritty", &["--version"], "0.17"),
                self.versioned("yazi", "Yazi", &["--version"], "26.5.6"),
                self.versioned("hyprpaper", "hyprpaper", &["--version"], "0.8.4"),
                self.versioned("awww", "awww", &["--version"], "0.12.1"),
            ],
        }
    }

    fn hyprland(&self) -> ComponentStatus {
        if !self.probe.executable("hyprctl") {
            return missing("hyprland", "Hyprland", "hyprctl was not found");
        }
        let version = self
            .probe
            .output("hyprctl", &["version", "-j"])
            .ok()
            .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
            .and_then(|value| {
                value
                    .get("tag")
                    .or_else(|| value.get("version"))
                    .and_then(serde_json::Value::as_str)
                    .map(normalize_version)
            });
        let available = version
            .as_deref()
            .is_some_and(|found| version_at_least(found, "0.56.2"));
        ComponentStatus {
            id: "hyprland".into(),
            display_name: "Hyprland".into(),
            availability: if available {
                Availability::Available
            } else {
                Availability::UnsupportedVersion
            },
            version,
            capabilities: if available {
                vec!["general".into(), "displays".into(), "keybindings".into()]
            } else {
                Vec::new()
            },
            notes: if available {
                Vec::new()
            } else {
                vec!["Helm requires Hyprland 0.56.2 or newer".into()]
            },
        }
    }

    fn versioned(
        &self,
        executable: &str,
        display_name: &str,
        arguments: &[&str],
        minimum: &str,
    ) -> ComponentStatus {
        if !self.probe.executable(executable) {
            return missing(executable, display_name, "executable was not found");
        }
        let version = self
            .probe
            .output(executable, arguments)
            .ok()
            .and_then(|output| extract_version(&output));
        let supported = version
            .as_deref()
            .is_some_and(|found| version_at_least(found, minimum));
        ComponentStatus {
            id: executable.into(),
            display_name: display_name.into(),
            availability: if supported {
                Availability::Available
            } else {
                Availability::UnsupportedVersion
            },
            version,
            capabilities: Vec::new(),
            notes: if supported {
                Vec::new()
            } else {
                vec![format!("Helm requires {display_name} {minimum} or newer")]
            },
        }
    }
}

fn missing(id: &str, display_name: &str, note: &str) -> ComponentStatus {
    ComponentStatus {
        id: id.into(),
        display_name: display_name.into(),
        availability: Availability::Missing,
        version: None,
        capabilities: Vec::new(),
        notes: vec![note.into()],
    }
}

fn extract_version(value: &str) -> Option<String> {
    value.split_whitespace().find_map(|token| {
        let candidate = normalize_version(token);
        candidate
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_digit())
            .then_some(candidate)
    })
}

fn normalize_version(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('v')
        .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '.')
        .split(['-', '+'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn version_at_least(found: &str, required: &str) -> bool {
    let parse = |version: &str| {
        version
            .split('.')
            .take(3)
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .chain(std::iter::repeat(0))
            .take(3)
            .collect::<Vec<_>>()
    };
    parse(found) >= parse(required)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct FakeProbe {
        environment: BTreeMap<String, String>,
        commands: BTreeMap<String, String>,
    }
    impl Probe for FakeProbe {
        fn environment(&self, name: &str) -> Option<String> {
            self.environment.get(name).cloned()
        }
        fn executable(&self, name: &str) -> bool {
            self.commands.contains_key(name)
        }
        fn output(&self, program: &str, _arguments: &[&str]) -> Result<String, Error> {
            self.commands
                .get(program)
                .cloned()
                .ok_or_else(|| Error::Probe("missing fixture".into()))
        }
    }

    #[test]
    fn discovers_supported_environment_deterministically() {
        let mut probe = FakeProbe::default();
        probe
            .environment
            .insert("XDG_CURRENT_DESKTOP".into(), "Hyprland".into());
        probe
            .commands
            .insert("hyprctl".into(), r#"{"tag":"v0.56.2"}"#.into());
        probe
            .commands
            .insert("waybar".into(), "Waybar v0.15.0".into());
        let report = DiscoveryService::new(probe).discover();
        assert_eq!(report.session, "Hyprland");
        assert_eq!(report.components[0].availability, Availability::Available);
        assert_eq!(report.components[1].availability, Availability::Available);
        assert_eq!(report.components[2].availability, Availability::Missing);
    }

    #[test]
    fn rejects_hyprland_before_patch_floor() {
        let mut probe = FakeProbe::default();
        probe
            .commands
            .insert("hyprctl".into(), r#"{"tag":"v0.56.1"}"#.into());
        let report = DiscoveryService::new(probe).discover();
        assert_eq!(
            report.components[0].availability,
            Availability::UnsupportedVersion
        );
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(version_at_least("0.56.10", "0.56.2"));
        assert!(!version_at_least("0.9.0", "0.15.0"));
    }
}
