use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{AdapterError, Result};

/// A portable terminal-oriented color palette shared by application adapters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub dark: bool,
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub normal: BTreeMap<String, String>,
    pub bright: BTreeMap<String, String>,
}

impl Theme {
    pub fn validate(&self) -> Result<()> {
        if !valid_id(&self.id) {
            return Err(AdapterError::Invalid(format!(
                "invalid theme id `{}`",
                self.id
            )));
        }
        for (name, color) in self.normal.iter().chain(&self.bright).chain([
            (&"background".to_owned(), &self.background),
            (&"foreground".to_owned(), &self.foreground),
            (&"cursor".to_owned(), &self.cursor),
        ]) {
            if !valid_color(color) {
                return Err(AdapterError::Invalid(format!(
                    "theme color `{name}` must be #RRGGBB"
                )));
            }
        }
        for group in [&self.normal, &self.bright] {
            for required in ANSI_NAMES {
                if !group.contains_key(required) {
                    return Err(AdapterError::Invalid(format!(
                        "theme is missing `{required}`"
                    )));
                }
            }
        }
        Ok(())
    }
}

pub const ANSI_NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

#[must_use]
pub fn builtins() -> Vec<Theme> {
    vec![palette(
        "helm-night",
        "Helm Night",
        true,
        "#101418",
        "#d8dee9",
        [
            "#2e3440", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0",
        ],
        [
            "#4c566a", "#d57780", "#b1d196", "#f0d399", "#8fafd1", "#c29ac1", "#96cedc", "#eceff4",
        ],
    )]
}

fn palette(
    id: &str,
    name: &str,
    dark: bool,
    background: &str,
    foreground: &str,
    normal: [&str; 8],
    bright: [&str; 8],
) -> Theme {
    Theme {
        id: id.into(),
        name: name.into(),
        dark,
        background: background.into(),
        foreground: foreground.into(),
        cursor: foreground.into(),
        normal: ANSI_NAMES
            .into_iter()
            .zip(normal)
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
        bright: ANSI_NAMES
            .into_iter()
            .zip(bright)
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_valid() {
        for theme in builtins() {
            theme.validate().expect("valid built-in theme");
        }
    }

    #[test]
    fn rejects_incomplete_palette() {
        let mut theme = builtins().remove(0);
        theme.normal.remove("red");
        assert!(theme.validate().is_err());
    }
}
