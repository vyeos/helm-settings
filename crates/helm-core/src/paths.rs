use std::{ffi::OsString, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XdgPaths {
    pub config_home: PathBuf,
    pub data_home: PathBuf,
    pub state_home: PathBuf,
    pub cache_home: PathBuf,
    pub runtime_directory: Option<PathBuf>,
}

impl XdgPaths {
    pub fn from_environment() -> Result<Self, &'static str> {
        Self::from_lookup(|name| std::env::var_os(name))
    }

    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<OsString>) -> Result<Self, &'static str> {
        let home = PathBuf::from(lookup("HOME").ok_or("HOME is not set")?);
        Ok(Self {
            config_home: lookup("XDG_CONFIG_HOME")
                .map_or_else(|| home.join(".config"), PathBuf::from),
            data_home: lookup("XDG_DATA_HOME")
                .map_or_else(|| home.join(".local/share"), PathBuf::from),
            state_home: lookup("XDG_STATE_HOME")
                .map_or_else(|| home.join(".local/state"), PathBuf::from),
            cache_home: lookup("XDG_CACHE_HOME").map_or_else(|| home.join(".cache"), PathBuf::from),
            runtime_directory: lookup("XDG_RUNTIME_DIR").map(PathBuf::from),
        })
    }

    #[must_use]
    pub fn helm_state(&self) -> PathBuf {
        self.state_home.join("helm-settings")
    }

    #[must_use]
    pub fn writable_roots(&self) -> Vec<PathBuf> {
        vec![self.config_home.clone(), self.data_home.clone()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn follows_xdg_defaults_and_overrides() {
        let values = BTreeMap::from([
            ("HOME", OsString::from("/home/test")),
            ("XDG_CONFIG_HOME", OsString::from("/configuration")),
        ]);
        let paths = XdgPaths::from_lookup(|name| values.get(name).cloned()).expect("paths");
        assert_eq!(paths.config_home, PathBuf::from("/configuration"));
        assert_eq!(paths.state_home, PathBuf::from("/home/test/.local/state"));
    }
}
