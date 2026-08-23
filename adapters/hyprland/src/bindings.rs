use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub modifiers: Vec<String>,
    pub key: String,
    pub dispatcher: String,
    pub argument: String,
    pub submap: String,
    pub description: String,
    pub options: BindingOptions,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BindingOptions {
    pub flags: BTreeSet<BindingFlag>,
    pub device: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingFlag {
    Repeat,
    Locked,
    Release,
    LongPress,
    Transparent,
    IgnoreModifiers,
    SubmapUniversal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Submap {
    pub name: String,
    pub reset_targets: Vec<String>,
    pub emergency_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingCollision {
    pub first: usize,
    pub second: usize,
    pub reason: String,
}

#[must_use]
pub fn validate_bindings(bindings: &[Binding]) -> Vec<BindingCollision> {
    let mut collisions = Vec::new();
    for (first_index, first) in bindings.iter().enumerate() {
        if reserved_vt(&first.modifiers, &first.key) {
            collisions.push(BindingCollision {
                first: first_index,
                second: first_index,
                reason: "Ctrl+Alt+F1…F12 is compositor-reserved".into(),
            });
        }
        for (second_index, second) in bindings.iter().enumerate().skip(first_index + 1) {
            if overlaps(first, second) {
                collisions.push(BindingCollision {
                    first: first_index,
                    second: second_index,
                    reason: "bindings overlap in key, modifiers, submap and device scope".into(),
                });
            }
        }
    }
    collisions
}

pub fn validate_submaps(submaps: &[Submap]) -> Result<(), Vec<String>> {
    let known = submaps
        .iter()
        .map(|submap| submap.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut errors = Vec::new();
    for submap in submaps {
        if submap.emergency_key.trim().is_empty() {
            errors.push(format!("{} has no emergency reset key", submap.name));
        }
        if submap.name != "reset" && !can_reach_reset(&submap.name, submaps, &known) {
            errors.push(format!("{} has no route to reset", submap.name));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn can_reach_reset(start: &str, submaps: &[Submap], known: &BTreeSet<&str>) -> bool {
    let edges = submaps
        .iter()
        .map(|submap| (submap.name.as_str(), submap.reset_targets.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let mut pending = vec![start];
    let mut visited = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if current == "reset" {
            return true;
        }
        if !visited.insert(current) {
            continue;
        }
        if let Some(targets) = edges.get(current) {
            pending.extend(
                targets
                    .iter()
                    .filter(|target| known.contains(target.as_str()) || target.as_str() == "reset")
                    .map(String::as_str),
            );
        }
    }
    false
}

fn overlaps(first: &Binding, second: &Binding) -> bool {
    let first_modifiers = normalized_modifiers(&first.modifiers);
    let second_modifiers = normalized_modifiers(&second.modifiers);
    let modifiers_overlap = first.options.flags.contains(&BindingFlag::IgnoreModifiers)
        || second.options.flags.contains(&BindingFlag::IgnoreModifiers)
        || first_modifiers == second_modifiers;
    let submaps_overlap = first.options.flags.contains(&BindingFlag::SubmapUniversal)
        || second.options.flags.contains(&BindingFlag::SubmapUniversal)
        || first.submap == second.submap;
    let devices_overlap = first.options.device.is_none()
        || second.options.device.is_none()
        || first.options.device == second.options.device;
    first.key.eq_ignore_ascii_case(&second.key)
        && modifiers_overlap
        && submaps_overlap
        && devices_overlap
}

fn normalized_modifiers(modifiers: &[String]) -> BTreeSet<String> {
    modifiers
        .iter()
        .map(|value| value.to_ascii_uppercase())
        .collect()
}

fn reserved_vt(modifiers: &[String], key: &str) -> bool {
    let normalized = normalized_modifiers(modifiers);
    normalized.contains("CTRL")
        && normalized.contains("ALT")
        && key
            .strip_prefix('F')
            .and_then(|value| value.parse::<u8>().ok())
            .is_some_and(|number| (1..=12).contains(&number))
}

impl Binding {
    #[must_use]
    pub fn to_lua(&self) -> String {
        let keys = if self.modifiers.is_empty() {
            self.key.clone()
        } else {
            format!("{}+{}", self.modifiers.join("+"), self.key)
        };
        format!(
            "hl.bind({:?}, {:?}, {{ description = {:?}, repeat = {}, locked = {}, release = {}, long_press = {}, transparent = {}, ignore_mods = {}, submap_universal = {} }}, {:?})",
            keys,
            self.dispatcher,
            self.description,
            self.has(BindingFlag::Repeat),
            self.has(BindingFlag::Locked),
            self.has(BindingFlag::Release),
            self.has(BindingFlag::LongPress),
            self.has(BindingFlag::Transparent),
            self.has(BindingFlag::IgnoreModifiers),
            self.has(BindingFlag::SubmapUniversal),
            self.argument
        )
    }

    fn has(&self, flag: BindingFlag) -> bool {
        self.options.flags.contains(&flag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding(key: &str) -> Binding {
        Binding {
            modifiers: vec!["SUPER".into()],
            key: key.into(),
            dispatcher: "exec".into(),
            argument: "alacritty".into(),
            submap: String::new(),
            description: "Helm: terminal".into(),
            options: BindingOptions::default(),
        }
    }
    #[test]
    fn detects_exact_collision() {
        assert_eq!(validate_bindings(&[binding("T"), binding("t")]).len(), 1);
    }
    #[test]
    fn validates_escape_graph() {
        let maps = vec![Submap {
            name: "resize".into(),
            reset_targets: vec!["reset".into()],
            emergency_key: "Escape".into(),
        }];
        assert!(validate_submaps(&maps).is_ok());
    }
    #[test]
    fn rejects_trapped_submap() {
        let maps = vec![Submap {
            name: "resize".into(),
            reset_targets: vec!["resize".into()],
            emergency_key: "Escape".into(),
        }];
        assert!(validate_submaps(&maps).is_err());
    }
}
