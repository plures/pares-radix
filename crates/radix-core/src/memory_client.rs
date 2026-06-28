//! Handler-facing memory interface (platform seam).
//!
//! These lightweight types are the interface the built-in handler procedures
//! (e.g. [`crate::handlers::OnMessage`]) use to recall and capture memories.
//! They are deliberately minimal — just `role`/`content`/`id` — and carry no
//! cognition logic. The concrete implementation (PluresLM) lives in the
//! cognition crate (`pares-agens-core`), which re-exports these types from
//! `pares_radix_core::memory` for backward compatibility.
//!
//! This mirrors the `subagent_spawn::SubAgentSpawner` seam established in
//! Stage S1: platform owns the trait + DTOs, cognition owns the impl.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A recalled memory record (handler interface type).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Unique memory identifier.
    pub id: String,
    /// Role associated with this memory (e.g. `"user"`, `"assistant"`).
    pub role: String,
    /// Text content of the memory.
    pub content: String,
}

/// A memory capture request submitted by a handler procedure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCapture {
    /// Role of the message being captured.
    pub role: String,
    /// Text content to store as a memory.
    pub content: String,
}

/// Simplified memory client interface used by the built-in handler procedures.
#[async_trait]
pub trait MemoryClient: Send + Sync {
    /// Recall up to `limit` memories matching `query`.
    async fn recall(&self, query: &str, limit: usize) -> Vec<Memory>;
    /// Capture a memory entry.
    async fn capture(&self, entry: MemoryCapture) -> Result<(), String>;
}
