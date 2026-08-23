//! Cooperative Waybar and Quickshell integrations.

#![forbid(unsafe_code)]

pub mod quickshell;
pub mod waybar;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl Position {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub position: Position,
    pub size: u16,
    pub spacing: u16,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            position: Position::Top,
            size: 32,
            spacing: 8,
            left: vec!["hyprland/workspaces".into(), "hyprland/window".into()],
            center: vec!["clock".into()],
            right: vec![
                "network".into(),
                "pulseaudio".into(),
                "battery".into(),
                "tray".into(),
            ],
        }
    }
}

impl Layout {
    pub fn validate(&self) -> Result<(), String> {
        if !(16..=256).contains(&self.size) {
            return Err("bar size must be between 16 and 256 pixels".into());
        }
        if self.spacing > 64 {
            return Err("bar spacing must not exceed 64 pixels".into());
        }
        for module in self.left.iter().chain(&self.center).chain(&self.right) {
            if module.is_empty()
                || !module.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'/' | b'#')
                })
            {
                return Err(format!("invalid bar module id `{module}`"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_is_valid() {
        Layout::default().validate().expect("valid layout");
    }

    #[test]
    fn rejects_command_like_module_ids() {
        let layout = Layout {
            left: vec!["clock; rm".into()],
            ..Layout::default()
        };
        assert!(layout.validate().is_err());
    }
}
