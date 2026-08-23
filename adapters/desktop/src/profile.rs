use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    Error, Result, defaults,
    rules::{self, WindowRule},
    startup::StartupEntry,
    wallpaper::{self, Wallpaper},
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub wallpapers: Vec<Wallpaper>,
    #[serde(default)]
    pub defaults: BTreeMap<String, String>,
    #[serde(default)]
    pub startup: Vec<StartupEntry>,
    #[serde(default)]
    pub window_rules: Vec<WindowRule>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileFile {
    pub path: PathBuf,
    pub content: String,
}

impl Profile {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(Error::Invalid("profile name cannot be empty".into()));
        }
        let mut ids = BTreeSet::new();
        for entry in &self.startup {
            entry.validate()?;
            if !ids.insert(&entry.id) {
                return Err(Error::Invalid(format!(
                    "duplicate startup id `{}`",
                    entry.id
                )));
            }
        }
        let mut names = BTreeSet::new();
        for rule in &self.window_rules {
            rule.validate()?;
            if !names.insert(&rule.name) {
                return Err(Error::Invalid(format!("duplicate rule `{}`", rule.name)));
            }
        }
        for item in &self.wallpapers {
            item.validate()?;
        }
        defaults::set_defaults("", &self.defaults)?;
        Ok(())
    }

    pub fn render_files(
        &self,
        config_home: &Path,
        mimeapps_source: &str,
        hyprpaper_source: &str,
        hyprland_source: &str,
        helm_init_source: &str,
    ) -> Result<Vec<ProfileFile>> {
        self.validate()?;
        let mut files = Vec::new();
        if !self.wallpapers.is_empty() {
            let managed_path = wallpaper::managed_path(config_home);
            files.push(ProfileFile {
                path: config_home.join("hypr/hyprpaper.conf"),
                content: wallpaper::integrate_source(hyprpaper_source, &managed_path),
            });
            files.push(ProfileFile {
                path: managed_path,
                content: wallpaper::render(&self.wallpapers)?,
            });
        }
        if !self.defaults.is_empty() {
            files.push(ProfileFile {
                path: config_home.join("mimeapps.list"),
                content: defaults::set_defaults(mimeapps_source, &self.defaults)?,
            });
        }
        for entry in &self.startup {
            files.push(ProfileFile {
                path: entry.path(config_home),
                content: entry.render()?,
            });
        }
        if !self.window_rules.is_empty() {
            if hyprland_source.is_empty() {
                return Err(Error::Unsupported(
                    "window-rule profiles require an existing hyprland.lua".into(),
                ));
            }
            let root_import = "pcall(require, \"helm-settings.init\")";
            let root = if hyprland_source.contains(root_import) {
                hyprland_source.to_owned()
            } else {
                append_line(hyprland_source, root_import)
            };
            files.push(ProfileFile {
                path: config_home.join("hypr/hyprland.lua"),
                content: root,
            });
            let import = "pcall(require, \"helm-settings.profile\")";
            let init = if helm_init_source.contains(import) {
                helm_init_source.to_owned()
            } else {
                append_line(helm_init_source, import)
            };
            files.push(ProfileFile {
                path: config_home.join("hypr/helm-settings/init.lua"),
                content: init,
            });
            files.push(ProfileFile {
                path: config_home.join("hypr/helm-settings/profile.lua"),
                content: rules::render(&self.window_rules)?,
            });
        }
        if files.is_empty() {
            return Err(Error::Invalid(
                "profile does not contain any settings".into(),
            ));
        }
        Ok(files)
    }
}

fn append_line(source: &str, line: &str) -> String {
    format!(
        "{}{separator}{line}\n",
        source,
        separator = if source.is_empty() || source.ends_with('\n') {
            ""
        } else {
            "\n"
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_rejects_duplicate_rule_names() {
        let rule = WindowRule {
            name: "dialogs".into(),
            class: Some("dialog".into()),
            title: None,
            workspace: None,
            floating: Some(true),
            opacity: None,
        };
        let profile = Profile {
            name: "Work".into(),
            window_rules: vec![rule.clone(), rule],
            ..Profile::default()
        };
        assert!(profile.validate().is_err());
    }

    #[test]
    fn profile_json_has_stable_shape() {
        let value = serde_json::to_value(Profile {
            name: "Work".into(),
            ..Profile::default()
        })
        .expect("JSON");
        assert!(value["startup"].is_array());
        assert!(value["window_rules"].is_array());
    }
}
