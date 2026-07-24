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
//!   2. Emits `SpineEvent::Inbound` to the pipeline (IO)
//!   3. Records execution timestamp (IO: state write)
//!
//! If you're tempted to add decision logic here, STOP.
//! Write it in .px first (C-DEV-001).

use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::spine::event::SpineEvent;
use crate::spine::pipeline::PipelineEmitter;
use crate::state::StateStore;
use crate::task_manager::TaskManager;

/// IO boundary: dispatches task execution prompts to the agent via spine pipeline.
///
/// Does NOT decide what to execute — that's `.px`.
/// Only performs the side-effect: injecting a task prompt as an Inbound event
/// into the same SpinePipeline that handles user messages.
pub struct TaskDispatcher {
    state: Arc<dyn StateStore>,
    pipeline_emitter: Option<PipelineEmitter>,
}

impl TaskDispatcher {
    pub fn new(state: Arc<dyn StateStore>) -> Self {
        Self {
            state,
            pipeline_emitter: None,
        }
    }

    #[must_use]
    pub fn with_pipeline_emitter(mut self, emitter: PipelineEmitter) -> Self {
        self.pipeline_emitter = Some(emitter);
        self
    }

    /// IO boundary: emit a task execution prompt into the spine pipeline.
    ///
    /// Called by the heartbeat after determining there's pending work.
    /// The prompt is injected as a SpineEvent::Inbound with source "task_executor",
    /// which flows through the same pipeline as user messages (model invoke → delivery).
    /// `chat_id`: the ORIGIN chat the task/promise was created in (from
    /// `Task.chat_id`, set at `TaskManager::create_task`). Autonomous redrives
    /// MUST carry this forward so the reply reaches the real conversation the
    /// promise was made in, instead of being injected with a synthetic
    /// `chat_id` that no channel adapter can deliver to. Pass `"0"` only for
    /// genuinely chat-less/system-internal tasks. Resolution from `task_id` to
    /// the real `chat_id` is the caller's responsibility (see
    /// `TaskDispatchActionHandler::dispatch_task`, which resolves it via
    /// `TaskManager::get_task`).
    pub fn dispatch(&self, task_id: &str, prompt: &str, chat_id: &str) -> bool {
        let Some(emitter) = &self.pipeline_emitter else {
            warn!("task_dispatcher: no pipeline emitter — cannot dispatch");
            return false;
        };

        info!(task_id = %task_id, chat_id = %chat_id, "task_dispatcher: emitting task to pipeline");

        // Spawn the emit as a background task since PipelineEmitter::emit is async
        let emitter = emitter.clone();
        let task_id_owned = task_id.to_string();
        let prompt_owned = prompt.to_string();
        let chat_id_owned = chat_id.to_string();
        tokio::spawn(async move {
            emitter
                .emit(SpineEvent::Inbound {
                    id: SpineEvent::new_id(),
                    source: "task_executor".into(),
                    chat_id: chat_id_owned,
                    sender: format!("task:{}", task_id_owned),
                    content: prompt_owned,
                    metadata: serde_json::json!({
                        "task_id": task_id_owned,
                        "autonomous": true,
                    }),
                })
                .await;
        });

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
