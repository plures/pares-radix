//! Plugin framework — turns pares-radix into an application platform.
//!
//! Plugins declare schemas, logic, tools, and optional UI via a
//! [`PluginManifest`].  The [`PluginRuntime`] loads manifests, registers
//! their schemas in PluresDB, exposes generic CRUD tools, and injects
//! schema context into the system prompt so the AI knows what data exists.

pub mod capability;
pub mod coding_agent;
pub mod crud;
pub mod error;
pub mod executor;
pub mod git_adapter;
pub mod hooks;
pub mod manifest;
pub mod platform_capabilities;
pub mod runtime;

pub use error::PluginError;
pub use executor::PluginCrudExecutor;
pub use manifest::*;
pub use platform_capabilities::{is_platform_capability, PLATFORM_CAPABILITIES};
pub use runtime::PluginRuntime;

pub use capability::{
    load_cid_from_toml, load_cid_from_toml_path, resolve_and_validate_capabilities,
    resolve_capabilities, validate_provider_surface, CapabilityBinding,
    CapabilityInterfaceDescriptor, CidNode, CidOperation,
};
