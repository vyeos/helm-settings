//! Lossless application and theme adapters.

#![forbid(unsafe_code)]

pub mod alacritty;
pub mod theme;
pub mod yazi;

use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("configuration is not supported: {0}")]
    Unsupported(String),
    #[error("invalid configuration: {0}")]
    Invalid(String),
    #[error("cannot read or write {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot parse TOML: {0}")]
    Toml(#[from] toml_edit::TomlError),
}

pub type Result<T> = std::result::Result<T, AdapterError>;
