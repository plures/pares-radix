//! Model Pool — dynamic model discovery, selection, and management.
//!
//! Replaces the static `ModelChain` with a dynamic pool that:
//! - Discovers models from configured providers at runtime
//! - Selects the best model for each task based on capabilities + RSI + cost
//! - Supports user enable/disable of individual models (immediate effect)
//! - Persists state to both config file and PluresDB (dual-mode)
//! - Caches discovery results for offline/degraded operation

pub mod discovery;
pub mod exclusion;
pub mod pool;
pub mod provider;
pub mod selection;
pub mod types;

pub use pool::ModelPool;
pub use types::*;
