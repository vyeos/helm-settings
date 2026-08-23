//! Shared orchestration for all Helm frontends.

#![forbid(unsafe_code)]

mod catalog;
mod discovery;
mod paths;

pub use catalog::foundation_catalog;
pub use discovery::{DiscoveryService, Probe, SystemProbe};
pub use helm_model as model;
pub use paths::XdgPaths;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the requested capability is not available")]
    Unsupported,
    #[error("environment probe failed: {0}")]
    Probe(String),
}
