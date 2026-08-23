//! Extended desktop adapters and atomic desired-state profiles.

#![forbid(unsafe_code)]

pub mod defaults;
pub mod profile;
pub mod rules;
pub mod startup;
pub mod wallpaper;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid desktop setting: {0}")]
    Invalid(String),
    #[error("required configuration is unsupported: {0}")]
    Unsupported(String),
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
