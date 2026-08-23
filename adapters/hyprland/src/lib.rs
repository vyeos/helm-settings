//! Hyprland 0.56.2+ adapter with a narrow generated-Lua ownership boundary.

#![forbid(unsafe_code)]

mod bindings;
mod config;
mod displays;
mod options;
mod runtime;

pub use bindings::{
    Binding, BindingCollision, BindingFlag, BindingOptions, Submap, validate_bindings,
    validate_submaps,
};
pub use config::{ConfigGeneration, IntegrationPlan, ManagedConfig, detect_generation};
pub use displays::{
    Display, DisplayLayout, DisplayTarget, DisplayValidation, FallbackRule, validate_display_layout,
};
pub use options::{CuratedOption, OptionCatalog, UpstreamDescription};
pub use runtime::{HyprlandRuntime, ProcessRuntime, RuntimeError};

pub const MINIMUM_VERSION: &str = "0.56.2";
