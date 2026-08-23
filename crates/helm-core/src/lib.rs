//! Shared orchestration for all Helm frontends.

#![forbid(unsafe_code)]

mod catalog;
mod discovery;

pub use catalog::foundation_catalog;
pub use discovery::{DiscoveryService, Probe, SystemProbe};
pub use helm_model as model;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the requested capability is not available")]
    Unsupported,
    #[error("environment probe failed: {0}")]
    Probe(String),
}
