#![allow(missing_docs)] // TODO: re-enable once API stabilizes
//! `pares-radix-core` — platform runtime for Pares Radix.
//!
//! This crate holds the platform-layer modules extracted from `pares-agens-core`
//! (Stage S2a physical split): the reactive event loop, procedure executor,
//! state store, model client/tool dispatcher abstractions, plugin framework,
//! spine, praxis gates, task model, and supporting infrastructure.
//!
//! Cognition modules (agent, cerebellum, delegation, memory, personality,
//! prompt_builder, model_chain, features, heartbeat) remain in
//! `pares-agens-core`, which depends on this crate.

/// Authentication helpers for external providers.
pub mod auth;
/// Chronos version timeline — causal audit trail for every data mutation.
pub mod chronos;
/// Message classifier contract (trait + DTOs; backends implemented by cognition).
pub mod classifier;
/// Shared slash command registry for all channel adapters.
pub mod commands;
/// Content-addressed deduplicating blob storage.
pub mod content_store;
/// Event types consumed and emitted by the reactive event loop.
pub mod event;
/// Reactive event loop executor with optimization safety enforcement.
pub mod executor;
/// Built-in event handler procedures.
pub mod handlers;
/// License key validation and Pro feature gates.
pub mod license;
/// Handler-facing memory interface (trait + DTOs; impl lives in cognition).
pub mod memory;
/// LLM model client and tool dispatcher abstractions.
pub mod model;

pub mod memory_client;

pub mod model_pool;
/// Optimization safety gates for runtime enforcement.
pub mod optimization;
#[cfg(feature = "otel")]
pub mod otel;
#[cfg(feature = "otel")]
pub mod otel_metrics;
/// Platform bridge to the PluresDB procedure engine (procedure/constraint execution).
pub mod pluresdb_bridge;
/// Praxis decision ledger and approval gate procedures.
pub mod praxis;
/// Procedure registry and priority-based event dispatch.
pub mod procedure;
/// Shipped default PluresLM procedure bundles (JSON library).
pub mod procedures;
/// Encrypted secret storage abstraction.
pub mod secrets;
/// First-run wizard configuration and state persistence.
pub mod setup;
/// Event source abstraction for polling inbound events.
pub mod source;
/// Key-value state store backed by PluresDB.
pub mod state;
/// Privacy-first anonymous telemetry aggregation primitives.
pub mod telemetry;

pub use event::Event;
pub use state::{InMemoryStateStore, PluresDbStateStore, StateStore};

/// Re-export the classifier contract types for ergonomic access.
pub use classifier::{
    ClassifierBackend, MessageClassification, MessageIntent, CLASSIFIER_SYSTEM_PROMPT,
};

/// Re-export PluresDB primitives for consumers that need the shared CrdtStore.
pub use pluresdb::{CrdtStore, MemoryStorage, SledStorage, StorageEngine};
/// Channel capability contracts for output rendering.
pub mod channel_contract;
/// Event spine — bridges pares-radix to PluresDB's AgensRuntime.
pub mod event_spine;
pub mod lifecycle;
/// Channel-specific output renderers.
pub mod renderers;
/// Tool execution governance — policies, timeouts, blocked-command filtering.
pub mod tool_governance;

pub mod model_download;
/// Plugin framework — application platform for schema-driven apps.
pub mod plugins;
/// Session persistence — save and restore conversation sessions.
pub mod session;
/// Praxis-driven task model — tracked units of work with completion conditions.
pub mod task;
/// Task manager — CRUD and lifecycle for tasks backed by PluresDB.
pub mod task_manager;

/// Built-in tools — task registry, etc.
pub mod tools;

/// Autonomous task executor — dispatches evaluable tasks to the agent loop.
pub mod task_executor;

pub mod error_display;
pub mod health;
pub mod pii_guard;
/// Bridge between compiled `.px` procedures and the core [`Procedure`] trait.
pub mod px_adapter;
pub mod shell_executor;
pub mod spine;
/// Platform-owned sub-agent spawn seam (trait + DTOs implemented by cognition).
pub mod subagent_spawn;
/// Thread engine — multi-topic conversation threading.
pub mod threading;
