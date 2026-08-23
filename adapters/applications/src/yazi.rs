use std::path::{Path, PathBuf};

use serde::Serialize;
use toml_edit::{DocumentMut, Item, Table, value};

use crate::{AdapterError, Result};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Flavor {
    pub id: String,
    pub path: PathBuf,
}

pub fn discover_flavors(config_home: &Path) -> Result<Vec<Flavor>> {
    let directory = config_home.join("yazi/flavors");
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(AdapterError::Io {
                path: directory,
                source,
            });
        }
    };
    let mut flavors = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some(id) = name.strip_suffix(".yazi")
            && path.join("flavor.toml").is_file()
        {
            flavors.push(Flavor {
                id: id.into(),
                path,
            });
        }
    }
    flavors.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(flavors)
}

pub fn select_flavor(source: &str, dark: &str, light: &str) -> Result<String> {
    for flavor in [dark, light] {
        if !valid_flavor_id(flavor) {
            return Err(AdapterError::Invalid(format!(
                "invalid Yazi flavor id `{flavor}`"
            )));
        }
    }
    let mut document = source.parse::<DocumentMut>()?;
    if document.get("flavor").is_none() {
        document["flavor"] = Item::Table(Table::new());
    }
    let table = document["flavor"]
        .as_table_mut()
        .ok_or_else(|| AdapterError::Unsupported("Yazi `flavor` must be a table".into()))?;
    table["dark"] = value(dark);
    table["light"] = value(light);
    Ok(document.to_string())
}

fn valid_flavor_id(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn losslessly_selects_flavors() {
        let result = select_flavor(
            "# personal\n[manager]\ncwd = { fg = \"red\" }\n",
            "foo",
            "bar",
        )
        .expect("edit");
        assert!(result.contains("# personal"));
        assert!(result.contains("dark = \"foo\""));
        assert!(result.contains("[manager]"));
    }

    #[test]
    fn discovers_only_complete_flavors() {
        let directory = tempfile::tempdir().expect("tempdir");
        let flavor = directory.path().join("yazi/flavors/ocean.yazi");
        std::fs::create_dir_all(&flavor).expect("directory");
        std::fs::write(flavor.join("flavor.toml"), "[manager]\n").expect("flavor");
        let result = discover_flavors(directory.path()).expect("discovery");
        assert_eq!(result[0].id, "ocean");
    }
}
