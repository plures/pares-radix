#![allow(missing_docs)] // TODO: re-enable once API stabilizes
//! `pares-agens-core` — cognition layer (agent, cerebellum, delegation, memory).
//!
//! As of Stage S2a the platform runtime (event loop, procedure executor, state,
//! model client, plugins, spine, praxis, tasks, …) lives in the
//! [`pares_radix_core`] crate. This crate depends on it and re-exports the
//! platform surface at its own root for backward compatibility with downstream
//! crates that still reference `pares_agens_core::<platform_item>` paths. Those
//! shims are transitional and will be removed once callers migrate to
//! `pares_radix_core::` directly (Stage S3).
//!
//! # Quick start
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use pares_agens_core::{
//!     executor::Executor,
//!     handlers::{OnMessage, OnStateChange, OnTimer},
//!     memory::MemoryClient,
//!     model::{ModelClient, ToolDispatcher},
//!     procedure::ProcedureRegistry,
//! };
//!
//! # #[tokio::main]
//! # async fn main() {
//! // Wire up your memory/model/tool implementations here.
//! // See the `handlers` module docs for the full interface.
//!
//! let on_timer = OnTimer::new();
//! let on_state_change = OnStateChange::new();
//!
//! let mut registry = ProcedureRegistry::new();
//! registry.register(Box::new(on_timer));
//! registry.register(Box::new(on_state_change));
//!
//! let executor = Executor::new(registry);
//! // executor.run(&source, 0).await;  // pass a real EventSource
//! # }
//! ```

// ---------------------------------------------------------------------------
// Platform crate re-export (Stage S2a). Downstream crates may use either
// `pares_radix_core::` directly or the transitional `pares_agens_core::`
// shims below.
// ---------------------------------------------------------------------------
pub use pares_radix_core;

// Platform module shims — keep `pares_agens_core::<platform>` paths resolving.
pub use pares_radix_core::auth;
pub use pares_radix_core::channel_contract;
pub use pares_radix_core::chronos;
pub use pares_radix_core::commands;
pub use pares_radix_core::content_store;
pub use pares_radix_core::error_display;
pub use pares_radix_core::event;
pub use pares_radix_core::event_spine;
pub use pares_radix_core::executor;
pub use pares_radix_core::handlers;
pub use pares_radix_core::health;
pub use pares_radix_core::license;
pub use pares_radix_core::lifecycle;
pub use pares_radix_core::model;
pub use pares_radix_core::model_download;
pub use pares_radix_core::model_pool;
pub use pares_radix_core::optimization;
#[cfg(feature = "otel")]
pub use pares_radix_core::otel;
#[cfg(feature = "otel")]
pub use pares_radix_core::otel_metrics;
pub use pares_radix_core::pii_guard;
pub use pares_radix_core::pluresdb_bridge;
pub use pares_radix_core::plugins;
pub use pares_radix_core::praxis;
pub use pares_radix_core::procedure;
pub use pares_radix_core::procedures;
pub use pares_radix_core::px_adapter;
pub use pares_radix_core::renderers;
pub use pares_radix_core::secrets;
pub use pares_radix_core::session;
pub use pares_radix_core::setup;
pub use pares_radix_core::shell_executor;
pub use pares_radix_core::source;
pub use pares_radix_core::spine;
pub use pares_radix_core::state;
pub use pares_radix_core::subagent_spawn;
pub use pares_radix_core::task;
pub use pares_radix_core::task_executor;
pub use pares_radix_core::task_manager;
pub use pares_radix_core::telemetry;
pub use pares_radix_core::threading;
pub use pares_radix_core::tool_governance;
pub use pares_radix_core::tools;

// Platform root re-export shims.
pub use pares_radix_core::Event;
pub use pares_radix_core::{InMemoryStateStore, PluresDbStateStore, StateStore};
pub use pares_radix_core::{CrdtStore, SledStorage, StorageEngine};

// ---------------------------------------------------------------------------
// Cognition modules (owned by this crate).
// ---------------------------------------------------------------------------

/// High-level agent abstraction and in-memory storage.
pub mod agent;
/// Cerebellum orchestrator — autorecall, routing, and pipeline execution.
pub mod cerebellum;
/// Local multi-agent delegation and concurrent sub-task execution.
pub mod delegation;
/// Feature-gate helpers over the license tier.
pub mod features;
/// PluresLM — native memory recall, capture, and context injection.
pub mod memory;
/// Model selection chain — BitNet → conscious → deep fallback.
pub mod model_chain;
/// Personality contracts — identity, tone, and behavioral rules.
pub mod personality;
/// Dynamic system prompt builder from personality contracts.
pub mod prompt_builder;
/// Heartbeat system — periodic proactive check-ins.
pub mod heartbeat;

pub use agent::Memory as AgentMemory;
pub use agent::{Agent, InMemory};
