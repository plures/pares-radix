//! Persisted-memory data structures (platform schema).
//!
//! These are the serde data-transfer types for the agent's persisted memory:
//! the [`entry::MemoryCategory`] taxonomy, [`entry::MemoryEntry`] (distilled
//! knowledge), [`entry::Exchange`] (a user→assistant input pair), and
//! [`entry::ChatTurn`] (raw conversation turns for context hydration).
//!
//! Ownership rationale: persisted-data *schemas* live in the platform
//! (`pares-radix-core`), alongside `state`, `license`, and `model`. The
//! cognition layer (`pares-agens-core`) re-exports these from here and adds
//! the behavior (embedding, recall, quality-gating, forgetting) on top. This
//! keeps the data shape platform-owned while the intelligence stays in
//! cognition — with no circular dependency.

/// Memory entry data structures and category taxonomy.
pub mod entry;

pub use entry::{ChatTurn, Exchange, MemoryCategory, MemoryEntry};
