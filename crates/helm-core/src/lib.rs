//! Shared orchestration for all Helm frontends.

#![forbid(unsafe_code)]

pub use helm_model as model;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the requested capability is not available")]
    Unsupported,
}
