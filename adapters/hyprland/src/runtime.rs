use std::process::Command;

use crate::{Binding, Display, OptionCatalog};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("hyprctl failed: {0}")]
    Command(String),
    #[error("Hyprland returned invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub trait HyprlandRuntime {
    fn descriptions(&self) -> Result<OptionCatalog, RuntimeError>;
    fn displays(&self) -> Result<Vec<Display>, RuntimeError>;
    fn bindings(&self) -> Result<Vec<Binding>, RuntimeError>;
    fn reload_and_verify(&self) -> Result<(), RuntimeError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessRuntime;

impl ProcessRuntime {
    fn command(arguments: &[&str]) -> Result<String, RuntimeError> {
        let output = Command::new("hyprctl")
            .args(arguments)
            .output()
            .map_err(|error| RuntimeError::Command(error.to_string()))?;
        if !output.status.success() {
            return Err(RuntimeError::Command(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl HyprlandRuntime for ProcessRuntime {
    fn descriptions(&self) -> Result<OptionCatalog, RuntimeError> {
        Ok(OptionCatalog::from_json(&Self::command(&[
            "descriptions",
        ])?)?)
    }
    fn displays(&self) -> Result<Vec<Display>, RuntimeError> {
        Ok(serde_json::from_str(&Self::command(&[
            "monitors", "all", "-j",
        ])?)?)
    }
    fn bindings(&self) -> Result<Vec<Binding>, RuntimeError> {
        let raw: Vec<RuntimeBinding> = serde_json::from_str(&Self::command(&["binds", "-j"])?)?;
        Ok(raw.into_iter().map(RuntimeBinding::into_binding).collect())
    }
    fn reload_and_verify(&self) -> Result<(), RuntimeError> {
        Self::command(&["reload"])?;
        let errors = Self::command(&["configerrors"])?;
        if errors.trim().is_empty() || errors.trim() == "ok" {
            Ok(())
        } else {
            Err(RuntimeError::Command(errors.trim().into()))
        }
    }
}

#[derive(serde::Deserialize)]
#[allow(clippy::struct_excessive_bools)] // This private type mirrors Hyprland's wire shape.
struct RuntimeBinding {
    modmask: u64,
    locked: bool,
    release: bool,
    repeat: bool,
    #[serde(rename = "longPress")]
    long_press: bool,
    submap: String,
    #[serde(rename = "submap_universal")]
    submap_universal: String,
    key: String,
    keycode: i64,
    description: String,
    dispatcher: String,
    arg: String,
}

impl RuntimeBinding {
    fn into_binding(self) -> Binding {
        let key = if self.key.is_empty() {
            format!("code:{}", self.keycode)
        } else {
            self.key
        };
        let mut flags = std::collections::BTreeSet::new();
        for (enabled, flag) in [
            (self.locked, crate::BindingFlag::Locked),
            (self.release, crate::BindingFlag::Release),
            (self.repeat, crate::BindingFlag::Repeat),
            (self.long_press, crate::BindingFlag::LongPress),
            (
                self.submap_universal == "true",
                crate::BindingFlag::SubmapUniversal,
            ),
        ] {
            if enabled {
                flags.insert(flag);
            }
        }
        Binding {
            modifiers: modifiers(self.modmask),
            key,
            dispatcher: self.dispatcher,
            argument: self.arg,
            submap: self.submap,
            description: self.description,
            options: crate::BindingOptions {
                flags,
                device: None,
            },
        }
    }
}

fn modifiers(mask: u64) -> Vec<String> {
    [(1, "SHIFT"), (4, "CTRL"), (8, "ALT"), (64, "SUPER")]
        .into_iter()
        .filter(|(bit, _)| mask & bit != 0)
        .map(|(_, name)| name.to_owned())
        .collect()
}
