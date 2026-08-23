use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StartupEntry {
    pub id: String,
    pub name: String,
    pub exec: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

impl StartupEntry {
    pub fn validate(&self) -> Result<()> {
        if !valid_id(&self.id) || self.name.is_empty() || self.exec.is_empty() {
            return Err(Error::Invalid(format!(
                "invalid startup entry `{}`",
                self.id
            )));
        }
        if self.name.contains(['\n', '\r']) || self.exec.contains(['\n', '\r']) {
            return Err(Error::Invalid(
                "startup values cannot contain newlines".into(),
            ));
        }
        Ok(())
    }

    pub fn render(&self) -> Result<String> {
        self.validate()?;
        Ok(format!(
            "[Desktop Entry]\nType=Application\nVersion=1.5\nName={}\nExec={}\nHidden={}\nX-Helm-Managed=true\n",
            self.name, self.exec, !self.enabled
        ))
    }

    #[must_use]
    pub fn path(&self, config_home: &Path) -> PathBuf {
        config_home
            .join("autostart")
            .join(format!("{}.desktop", self.id))
    }
}

const fn enabled() -> bool {
    true
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_entry_uses_standard_hidden_key() {
        let entry = StartupEntry {
            id: "io.example.Agent".into(),
            name: "Agent".into(),
            exec: "agent --quiet".into(),
            enabled: false,
        };
        assert!(entry.render().expect("render").contains("Hidden=true"));
    }
}
