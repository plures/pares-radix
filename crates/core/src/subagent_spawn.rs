//! Platform-owned sub-agent spawn seam.
//!
//! This module defines the **platform** (spine-facing) contract for spawning
//! and tracking sub-agent sessions. It intentionally has **no dependency on the
//! cognition `delegation` module** — the dependency points the other way:
//! `delegation::SubAgentManager` (cognition) implements [`SubAgentSpawner`],
//! while platform code (e.g. `spine::subagent_actor`) depends only on the trait
//! and DTOs defined here.
//!
//! This breaks the previous one-way `PLATFORM -> COGNITION` edge
//! (`spine/subagent_actor.rs -> crate::delegation`) by inverting it into a
//! `COGNITION -> PLATFORM` implementation of a platform-owned trait.

use std::time::Duration;

use async_trait::async_trait;

/// Options for spawning a sub-agent session (platform DTO).
///
/// Mirrors the builder shape of the cognition-side spawn options so platform
/// callers can configure a spawn without importing cognition types.
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    /// Optional label for this session.
    pub label: Option<String>,
    /// Timeout for this session (`None` = no timeout).
    pub timeout: Option<Duration>,
    /// Optional parent context summary.
    pub parent_context: Option<String>,
}

impl Default for SpawnOptions {
    fn default() -> Self {
        Self {
            label: None,
            timeout: Some(Duration::from_secs(30 * 60)), // 30 minutes default
            parent_context: None,
        }
    }
}

impl SpawnOptions {
    /// Set the timeout for this session.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Clear the timeout (run with no timeout).
    pub fn without_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }

    /// Set the display label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the parent context summary.
    pub fn with_parent_context(mut self, ctx: impl Into<String>) -> Self {
        self.parent_context = Some(ctx.into());
        self
    }

    /// The configured timeout, if any.
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    /// The configured label, if any.
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// The configured parent context, if any.
    pub fn parent_context(&self) -> Option<&str> {
        self.parent_context.as_deref()
    }
}

/// Status of a sub-agent session (platform copy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    /// Currently executing.
    Running,
    /// Completed successfully.
    Completed,
    /// Failed with an error.
    Failed(String),
    /// Timed out.
    TimedOut,
    /// Killed by the parent.
    Killed,
}

/// Minimal session information the platform reads back after a spawn.
#[derive(Debug, Clone)]
pub struct SpawnedInfo {
    /// Current status of the session.
    pub status: SessionStatus,
    /// Output text, if the session has produced any.
    pub output: Option<String>,
}

/// Platform-owned contract for spawning and tracking sub-agent sessions.
///
/// Cognition's `delegation::SubAgentManager` implements this trait, mapping its
/// internal session model onto the platform DTOs above. Platform code depends
/// only on `Arc<dyn SubAgentSpawner>`.
#[async_trait]
pub trait SubAgentSpawner: Send + Sync {
    /// Spawn a sub-agent for `agent` with `prompt`, returning the session id.
    ///
    /// Returns immediately; the session runs in the background.
    async fn spawn(&self, agent: &str, prompt: &str, options: SpawnOptions) -> String;

    /// Look up the current state of a previously-spawned session.
    ///
    /// Returns `None` if the session id is unknown.
    async fn get(&self, session_id: &str) -> Option<SpawnedInfo>;
}
