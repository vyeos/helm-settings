use std::collections::BTreeMap;

use crate::{Error, Result};

pub fn set_defaults(source: &str, defaults: &BTreeMap<String, String>) -> Result<String> {
    for (mime, desktop) in defaults {
        if !valid_mime(mime) || !valid_desktop_id(desktop) {
            return Err(Error::Invalid(format!(
                "invalid default association `{mime}` → `{desktop}`"
            )));
        }
    }
    let mut lines = source.lines().map(str::to_owned).collect::<Vec<_>>();
    let group = "[Default Applications]";
    let start = lines.iter().position(|line| line.trim() == group);
    let (start, end) = if let Some(start) = start {
        let end = lines[start + 1..]
            .iter()
            .position(|line| line.trim_start().starts_with('['))
            .map_or(lines.len(), |offset| start + 1 + offset);
        (start, end)
    } else {
        if !lines.is_empty() && lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
        lines.push(group.into());
        (lines.len() - 1, lines.len())
    };
    for (mime, desktop) in defaults {
        let value = format!("{mime}={desktop};");
        if let Some(index) = (start + 1..end).find(|index| {
            lines[*index]
                .split_once('=')
                .is_some_and(|(key, _)| key.trim() == mime)
        }) {
            lines[index] = value;
        } else {
            lines.insert(start + 1, value);
        }
    }
    let mut output = lines.join("\n");
    output.push('\n');
    Ok(output)
}

fn valid_mime(value: &str) -> bool {
    value.split_once('/').is_some_and(|(kind, subtype)| {
        !kind.is_empty()
            && !subtype.is_empty()
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'+' | b'-')
            })
    })
}

fn valid_desktop_id(value: &str) -> bool {
    value.ends_with(".desktop")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_unrelated_groups_and_comments() {
        let source = "# mine\n[Added Associations]\nimage/png=viewer.desktop;\n";
        let result = set_defaults(
            source,
            &BTreeMap::from([("text/plain".into(), "editor.desktop".into())]),
        )
        .expect("edit");
        assert!(result.contains("# mine"));
        assert!(result.contains("[Added Associations]"));
        assert!(result.contains("text/plain=editor.desktop;"));
    }
}
