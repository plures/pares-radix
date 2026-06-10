//! Autonomous task executor — IO boundary for task dispatch.
//!
//! This module is the RUST SIDE-EFFECT BOUNDARY ONLY.
//! All decision logic lives in .px procedures:
//!   - `commitment-detection.px` — detects promises in model responses
//!   - `task-steering.px` — classifies continuations vs new requests
//!   - `autonomous-dispatch.px` — decides which task to execute next
//!
//! This Rust module only does:
//!   1. Receives dispatch decisions from PxBridge
//!   2. Calls `EventSpine::emit_inbound_message()` (IO)
//!   3. Records execution timestamp (IO: state write)
//!
//! If you're tempted to add decision logic here, STOP.
//! Write it in .px first (C-DEV-001).

use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::event_spine::EventSpineHandle;
use crate::state::StateStore;
use crate::task_manager::TaskManager;

/// IO boundary: dispatches task execution prompts to the agent via event spine.
///
/// Does NOT decide what to execute — that's `.px`.
/// Only performs the side-effect: injecting a message into the agent loop.
pub struct TaskDispatcher {
    state: Arc<dyn StateStore>,
    event_spine: Option<EventSpineHandle>,
}

impl TaskDispatcher {
    pub fn new(state: Arc<dyn StateStore>) -> Self {
        Self {
            state,
            event_spine: None,
        }
    }

    #[must_use]
    pub fn with_event_spine(mut self, spine: EventSpineHandle) -> Self {
        self.event_spine = Some(spine);
        self
    }

    /// IO boundary: emit a task execution prompt into the agent loop.
    ///
    /// Called by PxBridge after `autonomous-dispatch.px` or `task-steering.px`
    /// produces a dispatch decision with a non-null prompt.
    pub fn dispatch(&self, task_id: &str, prompt: &str) -> bool {
        let Some(spine) = &self.event_spine else {
            warn!("task_dispatcher: no event spine — cannot dispatch");
            return false;
        };

        info!(task_id = %task_id, "task_dispatcher: emitting task to agent");
        spine.emit_inbound_message(0, "task_executor", prompt);
        true
    }

    /// IO boundary: record dispatch metadata to state.
    ///
    /// Called after successful dispatch to update execution tracking.
    pub async fn record_dispatch(&self, task_id: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        self.state
            .set(
                "task_executor/last_execution",
                serde_json::json!({
                    "task_id": task_id,
                    "timestamp": now,
                }),
            )
            .await;

        debug!(task_id = %task_id, timestamp = now, "task_dispatcher: recorded dispatch");
    }

    /// IO boundary: query evaluable tasks count (for heartbeat gating).
    ///
    /// Note: The .px procedure `evaluate_dispatch` does the actual filtering.
    /// This is only used for the heartbeat's "should I bother calling PxBridge?" fast-path.
    pub fn has_pending_work(task_manager: &TaskManager) -> bool {
        !task_manager.evaluable_tasks().is_empty()
    }
}
