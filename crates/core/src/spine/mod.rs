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
pub mod channel;
pub mod conversation;
pub mod dev_lifecycle_actions;
pub mod dispatcher;
pub mod model_selection_actions;
pub mod rsi_actions;
pub mod thread_actions;
pub mod topic_routing_actions;
pub mod event;
pub mod pipeline;
pub mod procedures;
pub mod reactive;
pub mod subagent_actor;
