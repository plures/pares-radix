//! Plugin error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin '{0}' already installed")]
    AlreadyInstalled(String),

    #[error("plugin '{0}' not found")]
    NotFound(String),

    #[error("invalid manifest: {0}")]
    InvalidManifest(String),

    #[error("schema registration failed: {0}")]
    SchemaRegistration(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("TOML parse error: {0}")]
    TomlParse(String),

    #[error("plugin '{plugin}' requires '{dependency}' which is not installed")]
    MissingDependency { plugin: String, dependency: String },

    #[error("circular dependency detected involving: {0}")]
    CircularDependency(String),

    /// A required provider capability (ADR-0022 §1/§3) has no installed provider
    /// whose `[capabilities.provided]` version satisfies the requested range.
    /// Required (vs. optional) capabilities block activation.
    ///
    /// Constructed by the Step 2 capability resolver (not yet wired in this
    /// manifest/schema-layer change); kept here as a real variant per ADR-0022
    /// so the schema and error surface land together.
    #[allow(dead_code)] // Constructed by the ADR-0022 Step 2 capability resolver.
    #[error("plugin '{plugin}' requires capability '{capability}' ({range}) which no installed provider satisfies")]
    UnsatisfiedCapability {
        plugin: String,
        capability: String,
        range: String,
    },

    /// More than one installed provider satisfies a required capability range and
    /// the binding-selection policy (ADR-0022 §4) could not pick one
    /// deterministically (no pin, tie on version/trust).
    ///
    /// Constructed by the Step 2 capability resolver (not yet wired in this
    /// manifest/schema-layer change).
    #[allow(dead_code)] // Constructed by the ADR-0022 Step 2 capability resolver.
    #[error("capability '{capability}' is ambiguous: multiple providers satisfy it ({})", candidates.join(", "))]
    AmbiguousCapability {
        capability: String,
        candidates: Vec<String>,
    },
}
