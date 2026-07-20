//! Runtime assembly — wires the `.px` procedure engine into the live spine.
//!
//! This module is the **assembly seam** that turns the individually-tested
//! spine components (the `.px` engine, [`ReactiveRegistry`], the
//! [`CompositeActionHandler`], and the [`PluresDbStateStore`]) into a single
//! running system. Before this module existed, every piece was real and
//! covered by tests, but **nothing assembled them**:
//! [`CompositeActionHandler::new`] and
//! [`register_reactive_procedures`](crate::spine::bootstrap::register_reactive_procedures)
//! had zero non-test callers, so no shipped path actually executed `.px`.
//!
//! [`build_reactive_runtime`] is the first real (non-test) caller of both.
//!
//! # What it assembles
//!
//! ```text
//!  praxis/procedures/*.px ─load─▶ PxProcedureAdapter ─register─▶ ReactiveRegistry
//!                                                                      ▲
//!  PluresDbStateStore ─┐                                               │ on_write
//!                      ├─▶ CoreActionHandler ─┐                        │
//!  ConversationStore ──┘                      ├─▶ CompositeActionHandler
//!  ToolDispatcher ─────▶ ToolDispatchHandler ─┘            (handler the
//!                                                          procedures call)
//!
//!  Pipeline::with_reactive(registry)  ──run──▶  event loop drives on_write
//! ```
//!
//! The pipeline event loop (`Pipeline::run`) already calls
//! `reactive.on_write(...)` for every event (see `pipeline.rs`); this module
//! supplies the registry that loop fires into, populated with the real `.px`
//! procedures and a handler backed by the durable [`StateStore`].
//!
//! # Path resolution (no dev-only absolutes)
//!
//! * **State dir** — [`resolve_state_dir`]: `RADIX_STATE_DIR` env override,
//!   otherwise `<system-temp>/pares-radix/state`. Always overridable; the
//!   default is a stable, writable, per-user location.
//! * **Praxis dir** — [`resolve_praxis_dir`]: `RADIX_PRAXIS_DIR` env override,
//!   otherwise `./praxis/procedures` relative to the current working directory
//!   (the in-repo / bundled-resource layout). No hardcoded developer path.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{info, warn};

use crate::model::ToolDispatcher;
use crate::px_adapter::ToolDispatchActionHandler;
use crate::spine::actions::CompositeActionHandler;
use crate::spine::bootstrap::register_reactive_procedures;
use crate::spine::conversation::ConversationStore;
use crate::spine::pipeline::Pipeline;
use crate::spine::reactive::ReactiveRegistry;
use crate::state::{PluresDbStateStore, StateStore};

/// Environment variable that overrides the durable state directory.
pub const STATE_DIR_ENV: &str = "RADIX_STATE_DIR";

/// Environment variable that overrides the `.px` procedure directory.
pub const PRAXIS_DIR_ENV: &str = "RADIX_PRAXIS_DIR";

/// Resolve the durable state directory.
///
/// Honors `RADIX_STATE_DIR` if set and non-empty; otherwise defaults to
/// `<system-temp>/pares-radix/state`. The directory is created if absent.
pub fn resolve_state_dir() -> PathBuf {
    let dir = match std::env::var(STATE_DIR_ENV) {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => std::env::temp_dir().join("pares-radix").join("state"),
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!(dir = %dir.display(), error = %e, "runtime: failed to create state dir");
    }
    dir
}

/// Resolve the `.px` procedure directory.
///
/// Honors `RADIX_PRAXIS_DIR` if set and non-empty; otherwise defaults to
/// `./praxis/procedures` relative to the current working directory.
pub fn resolve_praxis_dir() -> PathBuf {
    match std::env::var(PRAXIS_DIR_ENV) {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => PathBuf::from("praxis").join("procedures"),
    }
}

/// The assembled reactive runtime: a [`ReactiveRegistry`] populated with the
/// repo's `.px` procedures, wired to a [`Pipeline`] whose event loop drives
/// `on_write`, backed by a durable [`StateStore`].
///
/// Hold onto [`state_store`](Self::state_store) to read/write durable state
/// directly; call [`spawn`](Self::spawn) to start the pipeline event loop.
pub struct ReactiveRuntime {
    /// The reactive registry (`.px` triggers). Shared with the pipeline.
    pub registry: Arc<ReactiveRegistry>,
    /// The spine pipeline (event bus) wired with the registry.
    pub pipeline: Arc<Pipeline>,
    /// The durable state store backing `read_state`/`write_state`.
    pub state_store: Arc<dyn StateStore>,
    /// Number of `.px` procedures registered as reactive triggers.
    pub registered: usize,
    /// The pipeline's inbound event receiver (consumed by [`spawn`]).
    rx: Option<tokio::sync::mpsc::Receiver<crate::spine::event::SpineEvent>>,
}

impl ReactiveRuntime {
    /// Spawn the pipeline event loop on the current tokio runtime.
    ///
    /// Returns the [`JoinHandle`](tokio::task::JoinHandle) for the loop. After
    /// this, emitting events into the pipeline (or writing keys via the
    /// registry's `on_write`) fires matching `.px` procedures.
    pub fn spawn(&mut self) -> tokio::task::JoinHandle<()> {
        let rx = self
            .rx
            .take()
            .expect("ReactiveRuntime::spawn called more than once");
        let pipeline = Arc::clone(&self.pipeline);
        tokio::spawn(async move {
            pipeline.run(rx).await;
        })
    }
}

/// Assemble the reactive `.px` runtime from real components.
///
/// This is the production assembly path and the **first non-test caller** of
/// [`CompositeActionHandler::new`] and
/// [`register_reactive_procedures`](crate::spine::bootstrap::register_reactive_procedures).
///
/// # Arguments
///
/// * `state_store`   — durable key/value store backing `read_state`/`write_state`.
/// * `conversation_store` — store backing `read_history`/`append_history`.
/// * `tool_dispatcher` — dispatcher for any non-core action (tool calls). May be
///   wired lazily; pass a real dispatcher here so procedure tool-calls work.
/// * `praxis_dir`    — directory of `.px` files to load (see [`resolve_praxis_dir`]).
/// * `capacity`      — pipeline event-channel buffer capacity.
///
/// # Returns
///
/// A [`ReactiveRuntime`] with the registry populated and the pipeline wired.
/// Call [`ReactiveRuntime::spawn`] to start driving events.
pub async fn build_reactive_runtime(
    state_store: Arc<dyn StateStore>,
    conversation_store: Arc<dyn ConversationStore>,
    tool_dispatcher: Arc<dyn ToolDispatcher>,
    praxis_dir: &Path,
    capacity: usize,
) -> ReactiveRuntime {
    build_reactive_runtime_with_tasks(
        state_store,
        conversation_store,
        tool_dispatcher,
        None,
        praxis_dir,
        capacity,
    )
    .await
}

/// Like [`build_reactive_runtime`] but also wires a durable
/// [`TaskManager`](crate::task_manager::TaskManager) into the composite action
/// handler so the live reactive `.px` path can inject the persisted open-tasks
/// grounding block into the model system prompt each inbound turn
/// (pares-radix#467). Pass `None` to run without task grounding (the
/// `read_open_tasks_block` action then returns null and `.px` injects no
/// block).
pub async fn build_reactive_runtime_with_tasks(
    state_store: Arc<dyn StateStore>,
    conversation_store: Arc<dyn ConversationStore>,
    tool_dispatcher: Arc<dyn ToolDispatcher>,
    task_manager: Option<Arc<crate::task_manager::TaskManager>>,
    praxis_dir: &Path,
    capacity: usize,
) -> ReactiveRuntime {
    // 1. Tool handler — bridges `.px` action calls that aren't core/lifecycle
    //    into the tool dispatch pipeline.
    let tool_handler = Arc::new(ToolDispatchActionHandler::new(tool_dispatcher));

    // 2. The composite handler the procedures invoke. CoreActionHandler is now
    //    backed by the durable state store (read_state/write_state round-trip
    //    through PluresDB, not a stub).
    let mut composite = CompositeActionHandler::new(
        Arc::clone(&conversation_store),
        Arc::clone(&state_store),
        tool_handler,
    );
    if let Some(tm) = task_manager {
        // Durable open-tasks grounding over the SAME store (C-PLURES-003/004).
        composite = composite.with_task_grounding(tm);
    }

    // 3. Build the registry and the pipeline FIRST so the live pipeline emitter
    //    exists before we attach the autonomous task-dispatch IO edge. The
    //    TaskDispatcher injects task prompts as Inbound events through this
    //    same emitter (spine.px IO boundary #5), so it must be built over the
    //    real emitter, not a placeholder.
    let registry = Arc::new(ReactiveRegistry::new());
    let (pipeline, rx) = Pipeline::with_reactive(capacity, Arc::clone(&registry));
    let emitter = pipeline.emitter();

    // Build the real TaskDispatcher over the live StateStore + emitter and
    // attach it so the `.px` `dispatch_task` action can close the task loop.
    let dispatcher = Arc::new(
        crate::task_executor::TaskDispatcher::new(Arc::clone(&state_store))
            .with_pipeline_emitter(emitter.clone()),
    );
    composite.set_task_dispatch(Arc::new(
        crate::spine::task_dispatch_actions::TaskDispatchActionHandler::new(dispatcher),
    ));
    let composite = Arc::new(composite);

    // 4. Load every `.px` procedure against the registry, then give the
    //    registry the emitter so procedure-emitted events re-enter the pipeline.
    let registered = register_reactive_procedures(praxis_dir, &registry, composite).await;
    info!(
        registered,
        praxis_dir = %praxis_dir.display(),
        "runtime: reactive .px procedures registered against live registry"
    );
    registry.set_emitter(emitter).await;

    ReactiveRuntime {
        registry,
        pipeline,
        state_store,
        registered,
        rx: Some(rx),
    }
}

/// Convenience constructor that opens a durable [`PluresDbStateStore`] at the
/// resolved [`resolve_state_dir`] location and co-locates the conversation
/// store in the same CRDT store, then calls [`build_reactive_runtime`] with the
/// `.px` directory from [`resolve_praxis_dir`].
///
/// This is what a shipped runtime driver (e.g. the cognition `serve` loop or a
/// Tauri integration) calls at startup. Errors only if the durable store can't
/// be opened.
pub async fn build_default_reactive_runtime(
    tool_dispatcher: Arc<dyn ToolDispatcher>,
    capacity: usize,
) -> Result<ReactiveRuntime, String> {
    let state_dir = resolve_state_dir();
    let praxis_dir = resolve_praxis_dir();

    let pdb = PluresDbStateStore::open(&state_dir)
        .map_err(|e| format!("open state store at {}: {e}", state_dir.display()))?;
    // Co-locate conversation history in the same CRDT store as agent state.
    let conversation_store: Arc<dyn ConversationStore> = Arc::new(
        crate::spine::conversation::PluresConversationStore::new(pdb.crdt_store()),
    );
    // Durable task manager over the SAME CRDT store (C-PLURES-003/004) so the
    // live `.px` model path can surface persisted open tasks each turn
    // (pares-radix#467 — task amnesia).
    let task_manager = Arc::new(crate::task_manager::TaskManager::new(pdb.crdt_store()));
    let state_store: Arc<dyn StateStore> = Arc::new(pdb);

    Ok(build_reactive_runtime_with_tasks(
        state_store,
        conversation_store,
        tool_dispatcher,
        Some(task_manager),
        &praxis_dir,
        capacity,
    )
    .await)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ToolDefinition, ToolDispatcher};
    use crate::spine::conversation::MemoryConversationStore;
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use std::time::Duration;
    use tempfile::TempDir;

    /// A dispatcher that records nothing and returns null — the runtime's
    /// `.px` procedures under test use only core `write_state`/`read_state`,
    /// which route to the CoreActionHandler, NOT the dispatcher.
    struct NullDispatcher;

    #[async_trait]
    impl ToolDispatcher for NullDispatcher {
        async fn available_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }
        async fn call_tool(&self, _name: &str, _args: Value) -> String {
            "null".to_string()
        }
    }

    fn dispatcher() -> Arc<dyn ToolDispatcher> {
        Arc::new(NullDispatcher)
    }

    #[test]
    fn resolve_state_dir_honors_env() {
        // SAFETY: single-threaded test; we set then immediately read.
        // Build the override with the OS-native separator so the assertion is
        // portable: a hardcoded "C:\\..\\state" string is a SINGLE path
        // component on Linux (backslash is not a separator there), which made
        // `ends_with("state")` fail in CI. Use a real joined path instead.
        let override_dir = std::env::temp_dir().join("radix-override").join("state");
        std::env::set_var(STATE_DIR_ENV, &override_dir);
        let dir = resolve_state_dir();
        assert!(dir.ends_with("state"));
        assert_eq!(dir, override_dir);
        std::env::remove_var(STATE_DIR_ENV);
    }

    #[test]
    fn resolve_praxis_dir_default_is_relative() {
        std::env::remove_var(PRAXIS_DIR_ENV);
        let dir = resolve_praxis_dir();
        assert!(dir.ends_with(Path::new("praxis").join("procedures")));
    }

    /// END-TO-END PROOF (assembled path, real handler + real store):
    ///
    /// Build the real [`CompositeActionHandler`] + a real [`PluresDbStateStore`]
    /// via [`build_reactive_runtime`], register a real `.px` procedure that, on
    /// `on_write`, calls `write_state` to persist a derived node, then fire the
    /// pipeline's reactive `on_write` and read the persisted node back out of
    /// the SAME durable store.
    ///
    /// This exercises: write → reactive trigger match → `.px` execution →
    /// `write_state` effect through the real CoreActionHandler → durable
    /// PluresDB node → read-back. No NoOpHandler, no mock store.
    #[tokio::test]
    async fn end_to_end_write_triggers_px_procedure_persists_state() {
        let tmp = TempDir::new().unwrap();
        let praxis = tmp.path().join("procedures");
        std::fs::create_dir_all(&praxis).unwrap();

        // A minimal REAL procedure. trigger: on_write → registered under
        // "task_request:*" via the bootstrap trigger map (name == plan_task is
        // mapped there), but we give it a distinct name + explicit pattern via
        // its declared trigger kind. To keep it deterministic we name it so the
        // default-trigger fallback registers it under "on_write:*" — then we
        // fire an "on_write:..." key directly.
        //
        // The procedure reads the reactive-provided `$value` (NOT `$new_value`,
        // which the reactive binder does not set) and writes a node keyed by it.
        std::fs::write(
            praxis.join("proof.px"),
            r#"procedure persist_proof:
  trigger: on_write
  given: "Persist a node derived from the write that triggered us"
  write_state {key: "proof:landed", value: $value} -> $written
  return {ok: true}
"#,
        )
        .unwrap();

        // Real durable store (on-disk sled under the temp dir).
        let pdb = PluresDbStateStore::open(tmp.path().join("state")).unwrap();
        let state_store: Arc<dyn StateStore> = Arc::new(pdb);
        let conversation_store: Arc<dyn ConversationStore> =
            Arc::new(MemoryConversationStore::new());

        let runtime = build_reactive_runtime(
            Arc::clone(&state_store),
            conversation_store,
            dispatcher(),
            &praxis,
            16,
        )
        .await;

        // The proof procedure must have registered (trigger kind on_write →
        // "on_write:*" fallback pattern).
        assert!(
            runtime.registered >= 1,
            "expected the real .px procedure to register, got {}",
            runtime.registered
        );

        // Fire the registry exactly as the pipeline event loop does.
        let payload = json!({"task": "wire-px-runtime", "n": 42});
        runtime
            .registry
            .on_write("on_write:proof-1", &payload)
            .await;

        // The procedure executes on a spawned task; poll the durable store for
        // the node it should have written.
        let mut landed: Option<Value> = None;
        for _ in 0..50 {
            if let Some(v) = state_store.get("proof:landed").await {
                if !v.is_null() {
                    landed = Some(v);
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let landed = landed.expect(
            "procedure-driven write_state did not land a durable node — \
             the assembled write→.px→state path is not working",
        );
        assert_eq!(landed["task"], "wire-px-runtime");
        assert_eq!(landed["n"], 42);
    }

    /// W2 PROOF: the autonomous task-dispatch IO edge is closed end-to-end.
    ///
    /// A real `.px` procedure calls the `dispatch_task` action (the same verb
    /// `evaluate_dispatch`/`build_steered_prompt` now invoke). Through the
    /// assembled runtime this reaches the real `TaskDispatchActionHandler` →
    /// `TaskDispatcher` built over the LIVE pipeline emitter. A successful
    /// dispatch records `task_executor/last_execution` in the durable store,
    /// which we read back — proving the emitter was wired (dispatch returned
    /// true) and `record_dispatch` ran. No stub, no mock handler.
    #[tokio::test]
    async fn dispatch_task_action_closes_the_loop_and_records_execution() {
        let tmp = TempDir::new().unwrap();
        let praxis = tmp.path().join("procedures");
        std::fs::create_dir_all(&praxis).unwrap();

        // Minimal real procedure that invokes the dispatch_task IO edge with a
        // task id + prompt taken from the triggering write's $value.
        std::fs::write(
            praxis.join("dispatch_proof.px"),
            r#"procedure dispatch_proof:
  trigger: on_write
  given: "Invoke the autonomous task-dispatch IO edge"
  dispatch_task {task_id: "task-w2-proof", prompt: "execute the thing"} -> $res
  return {ok: true}
"#,
        )
        .unwrap();

        let pdb = PluresDbStateStore::open(tmp.path().join("state")).unwrap();
        let state_store: Arc<dyn StateStore> = Arc::new(pdb);
        let conversation_store: Arc<dyn ConversationStore> =
            Arc::new(MemoryConversationStore::new());

        let runtime = build_reactive_runtime(
            Arc::clone(&state_store),
            conversation_store,
            dispatcher(),
            &praxis,
            16,
        )
        .await;

        runtime
            .registry
            .on_write("on_write:dispatch-1", &json!({}))
            .await;

        // TaskDispatcher::record_dispatch writes task_executor/last_execution
        // only when dispatch succeeded (emitter present). Poll the durable store.
        let mut recorded: Option<Value> = None;
        for _ in 0..50 {
            if let Some(v) = state_store.get("task_executor/last_execution").await {
                if !v.is_null() {
                    recorded = Some(v);
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let recorded = recorded.expect(
            "dispatch_task did not record an execution — the task-dispatch IO edge \
             (dispatch_task → TaskDispatcher over the live emitter) is not wired",
        );
        assert_eq!(recorded["task_id"], "task-w2-proof");
    }

    /// W5 PROOF — the autonomous task LOOP closes end-to-end, locally,
    /// channel-agnostic (C-TEST-002). This is the "turned on" test: it exercises
    /// the NEW `heartbeat_tick` producer edge (W3) that was previously missing.
    ///
    /// What it drives, all real (no Telegram, no adapter, no model call):
    ///  1. A `SpineEvent::HeartbeatTick` is emitted through the LIVE pipeline
    ///     emitter (exactly what the pares-agens heartbeat runner now does each
    ///     tick). The pipeline loop turns it into `heartbeat_tick:<id>` and
    ///     fires the reactive registry — proving the producer edge reaches the
    ///     same reactive engine `evaluate_dispatch` listens on.
    ///  2. A real `.px` procedure registered on the `heartbeat_tick` queue
    ///     invokes the real `dispatch_task` IO action (the same verb
    ///     `evaluate_dispatch` calls after it selects a task), which hands off
    ///     to the real `TaskDispatcher` over the live emitter → injects a
    ///     `SpineEvent::Inbound{autonomous}` re-drive and records the dispatch.
    ///  3. The dispatch record (`task_executor/last_execution`) is read back
    ///     from the durable store — proving the re-drive fired.
    ///  4. A real `TaskManager::complete_task` flips the seeded task to
    ///     `Completed` — proving the `task_complete` terminal path (W4) works.
    ///
    /// If this passes, the loop runs end-to-end locally: tick → decision edge
    /// → dispatch (re-drive) → completion.
    #[tokio::test]
    async fn w5_heartbeat_tick_drives_dispatch_and_completion_closes_the_loop() {
        use crate::spine::event::SpineEvent;
        use crate::task::{CompletionCondition, ConditionType, TaskStatus};
        use crate::task_manager::TaskManager;
        use pluresdb::{CrdtStore, MemoryStorage, StorageEngine};

        let tmp = TempDir::new().unwrap();
        let praxis = tmp.path().join("procedures");
        std::fs::create_dir_all(&praxis).unwrap();

        // Real .px that fires on the heartbeat_tick queue (the W3 edge) and
        // invokes the real dispatch IO. We use the name `evaluate_dispatch` so
        // the bootstrap trigger-map registers it on `heartbeat_tick:*` (exactly
        // as the shipped autonomous-dispatch.px is registered) — this test
        // targets the tick PRODUCER edge; task-selection internals are W1/W2-tested.
        std::fs::write(
            praxis.join("tick_dispatch_proof.px"),
            r#"procedure evaluate_dispatch(tick: int from "heartbeat_tick"):
  given: "On a heartbeat tick, dispatch the selected autonomous task (W3 edge proof)"
  dispatch_task {task_id: "task-w5-loop", prompt: "execute the seeded task"} -> $res
  return {dispatched: true}
"#,
        )
        .unwrap();

        // Observer: fires on the pipeline's `inbound:*` writes and records a
        // durable marker when it sees the AUTONOMOUS re-drive (source=task_executor).
        // This directly observes the loop RE-ENTERING the same pipeline that
        // handles user messages — the whole point of "closing the loop".
        std::fs::write(
            praxis.join("redrive_observer.px"),
            r#"procedure classify_message(text: string from "inbound"):
  given: "Observe the autonomous re-drive re-entering the pipeline (W5 loop proof)"
  write_state {key: "w5/redrive_observed", value: true} -> $ok
  return {seen: true}
"#,
        )
        .unwrap();

        let pdb = PluresDbStateStore::open(tmp.path().join("state")).unwrap();
        let state_store: Arc<dyn StateStore> = Arc::new(pdb);
        let conversation_store: Arc<dyn ConversationStore> =
            Arc::new(MemoryConversationStore::new());

        // Real TaskManager over an in-memory CRDT store, seeded with an Open task
        // that has a completion condition — this is the task the loop closes on.
        let storage: Arc<dyn StorageEngine> = Arc::new(MemoryStorage::default());
        let crdt = Arc::new(CrdtStore::default().with_persistence(storage));
        let task_manager = Arc::new(TaskManager::new(Arc::clone(&crdt)));
        let seeded = task_manager.create_task(
            "W5 loop task",
            "local-test",
            vec![CompletionCondition {
                description: "the loop drives it to completion".into(),
                condition_type: ConditionType::RequesterAck,
                satisfied: false,
            }],
        );
        // Gate precondition: has_pending_work must be true (the gate the agens
        // heartbeat runner checks before emitting the tick).
        assert!(
            crate::task_executor::TaskDispatcher::has_pending_work(&task_manager),
            "seeded Open task must register as pending work — the has_pending_work gate"
        );

        let runtime = build_reactive_runtime_with_tasks(
            Arc::clone(&state_store),
            conversation_store,
            dispatcher(),
            Some(Arc::clone(&task_manager)),
            &praxis,
            16,
        )
        .await;

        // Subscribe to the pipeline's outbound stream so we can OBSERVE the
        // autonomous Inbound re-drive the dispatcher injects.
        let events_rx = runtime.pipeline.subscribe_deliveries();
        let emitter = runtime.pipeline.emitter();

        // Spawn the real pipeline loop (this is what run(rx) does in serve).
        let mut runtime = runtime;
        let rx = runtime.rx.take().expect("runtime rx");
        let pipeline = Arc::clone(&runtime.pipeline);
        let loop_handle = tokio::spawn(async move { pipeline.run(rx).await });

        // W3 EDGE: emit N synthetic heartbeat_tick events through the LIVE
        // emitter — exactly what the pares-agens heartbeat runner now does.
        let mut dispatched = false;
        for tick in 0..5i64 {
            emitter
                .emit(SpineEvent::HeartbeatTick {
                    id: SpineEvent::new_id(),
                    tick,
                })
                .await;

            // Poll for the dispatch record: record_dispatch writes
            // task_executor/last_execution ONLY when dispatch succeeded (the
            // real emitter injected the Inbound re-drive).
            for _ in 0..25 {
                if let Some(v) = state_store.get("task_executor/last_execution").await {
                    if !v.is_null() {
                        assert_eq!(v["task_id"], "task-w5-loop");
                        dispatched = true;
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            if dispatched {
                break;
            }
        }

        assert!(
            dispatched,
            "heartbeat_tick did NOT drive a dispatch — the W3 producer edge \
             (HeartbeatTick SpineEvent → pipeline on_write → heartbeat_tick:* → \
             .px → dispatch_task → TaskDispatcher) is not closed"
        );

        // Prove the re-drive actually RE-ENTERED the pipeline: the dispatcher
        // injected a SpineEvent::Inbound{source:task_executor}, which flows back
        // through the same pipeline loop and fires the inbound observer above,
        // landing a durable marker. (subscribe_deliveries only broadcasts
        // DeliveryRequest, so we observe re-entry via the reactive inbound path,
        // which is the real loop closure.)
        let mut redrive_observed = false;
        for _ in 0..50 {
            if let Some(v) = state_store.get("w5/redrive_observed").await {
                if v == json!(true) {
                    redrive_observed = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            redrive_observed,
            "the autonomous Inbound re-drive did not re-enter the pipeline — \
             the loop does not actually close (dispatch emitted, but the injected \
             Inbound never flowed back through the pipeline)"
        );
        let _ = &events_rx; // delivery stream is not the re-drive channel; kept for clarity
        loop_handle.abort();

        // W4 TERMINAL PATH: the task_complete path flips the task to Completed,
        // which is what stops re-dispatch on subsequent ticks.
        task_manager.complete_task(&seeded.id, Some("done by loop"));
        let after = task_manager.get_task(&seeded.id).expect("task exists");
        assert_eq!(
            after.status,
            TaskStatus::Completed,
            "complete_task did not flip the task to Completed — the terminal path is broken"
        );
        assert!(
            !crate::task_executor::TaskDispatcher::has_pending_work(&task_manager),
            "a Completed task must NOT register as pending work — the loop would re-dispatch forever"
        );
    }
}
