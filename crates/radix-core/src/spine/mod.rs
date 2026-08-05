//! Spine — the event-driven pipeline architecture (ADR-0001).
//!
//! The spine replaces the monolithic agent loop with a pipeline of
//! procedures connected by events. Channel adapters are thin I/O layers;
//! all logic lives in procedures that observe and emit events.
//!
//! # Architecture
//!
//! ```text
//! Channel Adapter → [Inbound] → Pipeline → [ModelRequest] → ModelInvoker
//!                                                              ↓
//!                              [DeliveryRequest] ← ResponseRouter ← [ModelResponse]
//!                                     ↓
//!                              Channel Adapter (broadcast subscriber)
//! ```

pub mod actions;
pub mod bootstrap;
pub mod briefing_actions;
pub mod channel;
pub mod chronos_watcher;
pub mod conversation;
pub mod dev_lifecycle_actions;
pub mod dispatcher;
pub mod epic_registry_actions;
pub mod event;
pub mod gui_launch_actions;
pub mod model_selection_actions;
pub mod pipeline;
pub mod plugin_privilege_actions;
pub mod procedures;
pub mod reactive;
pub mod repo_health_actions;
pub mod rsi_actions;
pub mod run_command_actions;
/// Runtime assembly — wires the `.px` engine + state store + handler into the live spine.
pub mod runtime;
pub mod shadow;
pub mod subagent_actor;
pub mod task_dashboard_actions;
pub mod task_dispatch_actions;
pub mod task_grounding_actions;
pub mod task_handoff_actions;
pub mod thread_actions;
pub mod topic_routing_actions;
pub mod worktask_actions;
