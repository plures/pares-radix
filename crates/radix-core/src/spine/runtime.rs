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

use async_trait::async_trait;
use pluresdb::CrdtStore;
use serde_json::Value;
use tracing::{info, warn};

use crate::model::{ModelClient, ToolDefinition, ToolDispatcher};
use crate::px_adapter::ToolDispatchActionHandler;
use crate::spine::actions::CompositeActionHandler;
use crate::spine::bootstrap::register_reactive_procedures;
use crate::spine::conversation::ConversationStore;
use crate::spine::pipeline::Pipeline;
use crate::spine::procedures::commitment_detector::CommitmentDetector;
use crate::spine::procedures::history_recorder::HistoryRecorder;
use crate::spine::procedures::inbound_router::InboundRouter;
use crate::spine::procedures::model_invoker::ModelInvoker;
use crate::spine::procedures::response_router::ResponseRouter;
use crate::spine::procedures::tool_executor::ToolExecutor;
use crate::spine::reactive::ReactiveRegistry;
use crate::state::{PluresDbStateStore, StateStore};
use crate::task_manager::TaskManager;
use crate::tools::TaskRegistryTool;

/// Environment variable that overrides the durable state directory.
pub const STATE_DIR_ENV: &str = "RADIX_STATE_DIR";

/// Environment variable that overrides the `.px` procedure directory.
pub const PRAXIS_DIR_ENV: &str = "RADIX_PRAXIS_DIR";

/// A [`ToolDispatcher`] wrapper that adds built-in `task_*` tools backed by a
/// durable [`TaskManager`] while delegating all other tools to an inner
/// dispatcher.
struct TaskAwareToolDispatcher {
    inner: Arc<dyn ToolDispatcher>,
    task_registry: Arc<TaskRegistryTool>,
}

impl TaskAwareToolDispatcher {
    fn new(inner: Arc<dyn ToolDispatcher>, task_manager: Arc<TaskManager>) -> Self {
        Self {
            inner,
            task_registry: Arc::new(TaskRegistryTool::new(task_manager)),
        }
    }
}

#[async_trait]
impl ToolDispatcher for TaskAwareToolDispatcher {
    async fn available_tools(&self) -> Vec<ToolDefinition> {
        let mut tools = TaskRegistryTool::tool_definitions();
        let mut seen: std::collections::HashSet<String> =
            tools.iter().map(|tool| tool.name.clone()).collect();

        for tool in self.inner.available_tools().await {
            if seen.insert(tool.name.clone()) {
                tools.push(tool);
            }
        }

        tools
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> String {
        if TaskRegistryTool::handles_tool(name) {
            return self.task_registry.call(name, arguments).await;
        }

        self.inner.call_tool(name, arguments).await
    }
}

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
    build_reactive_runtime_with_subagent(
        state_store,
        conversation_store,
        tool_dispatcher,
        None,
        None,
        praxis_dir,
        capacity,
    )
    .await
}

/// Like [`build_reactive_runtime`] but wires the optional durable
/// [`TaskManager`](crate::task_manager::TaskManager) grounding (pares-radix#467)
/// AND the optional [`SubagentActor`] task-completion seam.
///
/// * `task_manager` — when `Some`, the composite handler injects the persisted
///   open-tasks grounding block into the model system prompt each inbound turn.
///   Pass `None` to run without task grounding (`read_open_tasks_block` returns
///   null and `.px` injects no block).
/// * `subagent` — when `Some((spawner, task_manager))`, the runtime constructs
///   a [`SubagentActor::with_task_manager`] so that (a) `.px` `spawn_subagent`
///   calls reach a real spawner and (b) on the final stage completing,
///   `finalize_task` drives the owning [`TaskManager`] `Task` terminal. When
///   `None`, subagent actions error at call time as before.
pub async fn build_reactive_runtime_with_subagent(
    state_store: Arc<dyn StateStore>,
    conversation_store: Arc<dyn ConversationStore>,
    tool_dispatcher: Arc<dyn ToolDispatcher>,
    task_manager: Option<Arc<crate::task_manager::TaskManager>>,
    subagent: Option<(
        Arc<dyn crate::subagent_spawn::SubAgentSpawner>,
        Arc<crate::task_manager::TaskManager>,
    )>,
    praxis_dir: &Path,
    capacity: usize,
) -> ReactiveRuntime {
    // 1. Tool handler — bridges `.px` action calls that aren't core/lifecycle
    //    into the tool dispatch pipeline.
    let tool_handler = Arc::new(ToolDispatchActionHandler::new(tool_dispatcher));

    // 2. Build the registry first — the SubagentActor needs a handle to it so
    //    completion writes (`stage_complete:*`) re-enter the reactive system.
    let registry = Arc::new(ReactiveRegistry::new());

    // 3. The composite handler the procedures invoke. CoreActionHandler is now
    //    backed by the durable state store (read_state/write_state round-trip
    //    through PluresDB, not a stub). If a spawner + task manager are
    //    supplied, wire the SubagentActor so the task-completion seam is live.
    let mut composite_inner = CompositeActionHandler::new(
        Arc::clone(&conversation_store),
        Arc::clone(&state_store),
        tool_handler,
    );
    // Keep one shared durable TaskManager for BOTH task-grounding reads and
    // autonomous-dispatch writes (read_evaluable_tasks/mark_task_in_progress).
    // This preserves a single task store (C-PLURES-003/004).
    let dispatch_task_manager = task_manager.clone();
    if let Some(tm) = task_manager {
        // Durable open-tasks grounding over the SAME store (C-PLURES-003/004).
        composite_inner = composite_inner.with_task_grounding(tm);
    }
    if let Some((spawner, task_manager)) = subagent {
        let actor = Arc::new(crate::spine::subagent_actor::SubagentActor::with_task_manager(
            spawner,
            Arc::clone(&registry),
            task_manager,
        ));
        composite_inner.set_subagent_actor(actor);
        info!("runtime: SubagentActor wired with TaskManager — task-completion seam live");
    }

    // Build the pipeline + emitter BEFORE attaching the autonomous task-dispatch
    // IO edge: the TaskDispatcher injects task prompts as Inbound events through
    // this same emitter (spine.px IO boundary #5), so it must be built over the
    // real emitter, not a placeholder.
    let (pipeline, rx) = Pipeline::with_reactive(capacity, Arc::clone(&registry));
    let emitter = pipeline.emitter();

    // Build the real TaskDispatcher over the live StateStore + emitter and
    // attach it so the `.px` `dispatch_task` action can close the task loop.
    let dispatcher = Arc::new(
        crate::task_executor::TaskDispatcher::new(Arc::clone(&state_store))
            .with_pipeline_emitter(emitter.clone()),
    );
    composite_inner.set_task_dispatch(Arc::new(
        crate::spine::task_dispatch_actions::TaskDispatchActionHandler::new(
            dispatcher,
            dispatch_task_manager,
        ),
    ));
    let composite = Arc::new(composite_inner);

    // 4. Load every `.px` procedure against the (already-built) registry, then
    //    give the registry the emitter so procedure-emitted events re-enter the
    //    pipeline.
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
/// This is the low-level reactive-only constructor. Shipped agent runtimes that
/// need model invocation, task retention, and built-in task tools should prefer
/// [`build_default_task_aware_runtime`]. Errors only if the durable store can't
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

    Ok(build_reactive_runtime_with_subagent(
        state_store,
        conversation_store,
        tool_dispatcher,
        Some(task_manager),
        None,
        &praxis_dir,
        capacity,
    )
    .await)
}

/// Configuration for [`build_task_aware_runtime`].
///
/// Groups the assembly inputs to keep the constructor within the argument-count
/// lint limit while remaining explicit at call sites.
pub struct TaskAwareRuntimeConfig {
    /// Durable state store (PluresDB-backed in production).
    pub state_store: Arc<dyn StateStore>,
    /// Conversation history store.
    pub conversation_store: Arc<dyn ConversationStore>,
    /// Language model client used for inference.
    pub model_client: Arc<dyn ModelClient>,
    /// Outer tool dispatcher; `task_*` built-ins are layered on top.
    pub tool_dispatcher: Arc<dyn ToolDispatcher>,
    /// CRDT store used to persist durable tasks.
    pub task_store: Arc<CrdtStore>,
    /// Optional system prompt injected at the start of every model turn.
    pub system_prompt: Option<String>,
    /// Directory containing `.px` procedure files.
    pub praxis_dir: PathBuf,
    /// Event-channel capacity for the pipeline.
    pub capacity: usize,
}

/// Assemble a full task-aware spine runtime.
///
/// In addition to the reactive `.px` registry, this registers the core spine
/// procedures needed for a live conversational agent:
/// - inbound routing
/// - durable history recording
/// - model invocation
/// - tool execution
/// - response routing
/// - commitment detection
///
/// Open tasks are injected into every model turn and `task_*` tools are exposed
/// through a built-in dispatcher wrapper, so obligations survive vague follow-up
/// turns and process restarts.
pub async fn build_task_aware_runtime(config: TaskAwareRuntimeConfig) -> ReactiveRuntime {
    let TaskAwareRuntimeConfig {
        state_store,
        conversation_store,
        model_client,
        tool_dispatcher,
        task_store,
        system_prompt,
        praxis_dir,
        capacity,
    } = config;
    let task_manager = Arc::new(TaskManager::new(task_store));
    let task_dispatcher: Arc<dyn ToolDispatcher> = Arc::new(TaskAwareToolDispatcher::new(
        tool_dispatcher,
        Arc::clone(&task_manager),
    ));

    let runtime = build_reactive_runtime(
        state_store,
        Arc::clone(&conversation_store),
        Arc::clone(&task_dispatcher),
        &praxis_dir,
        capacity,
    )
    .await;

    let pipeline = Arc::clone(&runtime.pipeline);

    pipeline
        .register(Arc::new(InboundRouter::with_reactive(Arc::clone(
            &runtime.registry,
        ))))
        .await;
    pipeline
        .register(Arc::new(HistoryRecorder::new(Arc::clone(
            &conversation_store,
        ))))
        .await;

    let invoker = if let Some(prompt) = system_prompt {
        ModelInvoker::with_system_prompt(model_client, Arc::clone(&task_dispatcher), prompt)
    } else {
        ModelInvoker::new(model_client, Arc::clone(&task_dispatcher))
    }
    .with_conversation_store(Arc::clone(&conversation_store))
    .with_task_manager(Arc::clone(&task_manager));

    pipeline.register(Arc::new(invoker)).await;
    pipeline
        .register(Arc::new(ToolExecutor::new(Arc::clone(&task_dispatcher))))
        .await;
    pipeline.register(Arc::new(ResponseRouter)).await;
    pipeline
        .register(Arc::new(CommitmentDetector::new(task_manager)))
        .await;

    runtime
}

/// Convenience constructor for the shipped task-aware agent runtime.
///
/// Uses the default durable PluresDB-backed state and conversation stores,
/// co-locates task persistence in the same CRDT store, and registers the core
/// conversational pipeline with durable task grounding enabled.
pub async fn build_default_task_aware_runtime(
    model_client: Arc<dyn ModelClient>,
    tool_dispatcher: Arc<dyn ToolDispatcher>,
    capacity: usize,
    system_prompt: Option<String>,
) -> Result<ReactiveRuntime, String> {
    let state_dir = resolve_state_dir();
    let praxis_dir = resolve_praxis_dir();

    let pdb = PluresDbStateStore::open(&state_dir)
        .map_err(|e| format!("open state store at {}: {e}", state_dir.display()))?;
    let task_store = pdb.crdt_store();
    let conversation_store: Arc<dyn ConversationStore> = Arc::new(
        crate::spine::conversation::PluresConversationStore::new(Arc::clone(&task_store)),
    );
    let state_store: Arc<dyn StateStore> = Arc::new(pdb);

    Ok(build_task_aware_runtime(TaskAwareRuntimeConfig {
        state_store,
        conversation_store,
        model_client,
        tool_dispatcher,
        task_store,
        system_prompt,
        praxis_dir,
        capacity,
    })
    .await)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChatMessage, ChatOptions, ModelCompletion, ToolDefinition, ToolDispatcher};
    use crate::spine::conversation::MemoryConversationStore;
    use crate::spine::event::SpineEvent;
    use crate::task_manager::TaskManager;
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::sync::Mutex;

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

    struct CapturingModel {
        seen_messages: Arc<Mutex<Vec<ChatMessage>>>,
    }

    #[async_trait]
    impl ModelClient for CapturingModel {
        async fn complete(
            &self,
            messages: &[ChatMessage],
            _tools: &[ToolDefinition],
            _options: &ChatOptions,
        ) -> Result<ModelCompletion, String> {
            *self.seen_messages.lock().await = messages.to_vec();
            Ok(ModelCompletion {
                content: Some("ok".into()),
                tool_calls: vec![],
                logprobs: None,
                model: Some("test".into()),
            })
        }
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

    /// MILESTONE REACTIVE PROOF (assembled path, real handler + real store):
    ///
    /// A `milestone:<id>` write is the dashboard signal. This dogfoods the
    /// target architecture: PluresDB is the source of truth, a WRITE drives a
    /// `.px` procedure (LOGIC), and the procedure performs a durable IO
    /// side-effect via `write_state` (the pure-praxis analog of the dashboard
    /// freeze). No cron, no daemon, no mock spine -- the loop is real and
    /// in-process.
    ///
    /// The procedure is named `dashboard_milestone` so the bootstrap name-map
    /// routes it to the `milestone:*` trigger pattern (see bootstrap.rs). We
    /// then prove pattern discipline: a `milestone:` write REACTS; a
    /// `progress:` write (history, not a dashboard signal) does NOT.
    #[tokio::test]
    async fn milestone_write_triggers_dashboard_px_procedure_locally() {
        let tmp = TempDir::new().unwrap();
        let praxis = tmp.path().join("procedures");
        std::fs::create_dir_all(&praxis).unwrap();

        // REAL procedure: on a milestone write, persist the frozen dashboard
        // node derived from the triggering write's `$value`. LOGIC in the
        // procedure; the SIDE EFFECT is a durable state write.
        std::fs::write(
            praxis.join("dashboard_milestone.px"),
            "procedure dashboard_milestone:\n  trigger: on_write\n  given: \"Freeze the dashboard node from a milestone write\"\n  write_state {key: \"dashboard:frozen\", value: $value} -> $frozen\n  return {ok: true}\n",
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

        assert!(
            runtime.registered >= 1,
            "expected dashboard_milestone .px to register under milestone:*, got {}",
            runtime.registered
        );

        // (1) A NON-milestone write must NOT trigger the procedure.
        let progress = json!({"task": "some-task", "text": "history entry"});
        runtime.registry.on_write("progress:p-1", &progress).await;
        // Give any (erroneous) spawned reaction a chance, then assert nothing.
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert!(
            state_store.get("dashboard:frozen").await.is_none_or(|v| v.is_null()),
            "progress: write must NOT trigger the milestone dashboard procedure"
        );

        // (2) A milestone write MUST trigger the procedure -> durable IO.
        let milestone = json!({
            "task": "radix_milestone_reactive_proof",
            "text": "milestone reactive flow proven locally",
            "created_at": "2026-07-20T05:00:00Z"
        });
        runtime.registry.on_write("milestone:m-1", &milestone).await;

        let mut landed: Option<Value> = None;
        for _ in 0..50 {
            if let Some(v) = state_store.get("dashboard:frozen").await {
                if !v.is_null() {
                    landed = Some(v);
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let landed = landed.expect(
            "milestone: write did not drive the .px procedure to a durable \
             dashboard node -- the reactive spine loop is not working",
        );
        assert_eq!(landed["task"], "radix_milestone_reactive_proof");
        assert_eq!(landed["text"], "milestone reactive flow proven locally");
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

        let runtime = build_reactive_runtime_with_subagent(
            Arc::clone(&state_store),
            conversation_store,
            dispatcher(),
            Some(Arc::clone(&task_manager)),
            None,
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

    #[tokio::test]
    async fn task_aware_dispatcher_exposes_and_executes_task_tools() {
        let pdb = PluresDbStateStore::in_memory();
        let manager = Arc::new(TaskManager::new(pdb.crdt_store()));
        let dispatcher = TaskAwareToolDispatcher::new(dispatcher(), Arc::clone(&manager));

        let tools = dispatcher.available_tools().await;
        assert!(
            tools.iter().any(|tool| tool.name == "task_create"),
            "task_create should be exposed to the model"
        );

        let created = dispatcher
            .call_tool(
                "task_create",
                json!({"description": "Investigate the Telegram timeout"}),
            )
            .await;
        let created_json: Value =
            serde_json::from_str(&created).expect("task_create should return JSON");
        assert_eq!(created_json["status"], "created");
        assert_eq!(manager.open_tasks().len(), 1);
    }

    #[tokio::test]
    async fn task_aware_runtime_injects_persisted_tasks_into_model_turns() {
        let tmp = TempDir::new().unwrap();
        let praxis = tmp.path().join("procedures");
        std::fs::create_dir_all(&praxis).unwrap();

        let pdb = PluresDbStateStore::in_memory();
        let task_store = pdb.crdt_store();
        let state_store: Arc<dyn StateStore> = Arc::new(pdb);
        let conversation_store: Arc<dyn ConversationStore> =
            Arc::new(MemoryConversationStore::new());
        let seen_messages = Arc::new(Mutex::new(Vec::new()));
        let model: Arc<dyn ModelClient> = Arc::new(CapturingModel {
            seen_messages: Arc::clone(&seen_messages),
        });

        let mut runtime = build_task_aware_runtime(TaskAwareRuntimeConfig {
            state_store,
            conversation_store,
            model_client: model,
            tool_dispatcher: dispatcher(),
            task_store: Arc::clone(&task_store),
            system_prompt: Some("system prompt".into()),
            praxis_dir: praxis.to_path_buf(),
            capacity: 16,
        })
        .await;

        // Create a second TaskManager over the same durable store to simulate an
        // external writer (for example, commitment detection on a prior turn).
        let manager = Arc::new(TaskManager::new(task_store));
        manager.create_task(
            "Finish the follow-up investigation",
            "chat-1",
            vec![], // no completion conditions needed for this grounding test
        );

        let mut deliveries = runtime.pipeline.subscribe_deliveries();
        let handle = runtime.spawn();

        runtime
            .pipeline
            .emitter()
            .emit(SpineEvent::ModelRequest {
                id: SpineEvent::new_id(),
                source: "telegram".into(),
                chat_id: "chat-1".into(),
                sender: "user".into(),
                content: "try again".into(),
                system_prompt: None,
                metadata: json!({}),
            })
            .await;

        let delivered = tokio::time::timeout(Duration::from_secs(1), deliveries.recv())
            .await
            .expect("delivery request should be emitted")
            .expect("delivery broadcast should succeed");
        assert_eq!(delivered.event_type(), "delivery_request");

        let messages = seen_messages.lock().await.clone();
        assert!(
            messages.iter().any(|message| {
                message.role == "system"
                    && message
                        .content
                        .contains("Finish the follow-up investigation")
            }),
            "persisted open task should be injected into the model context"
        );

        handle.abort();
    }
}
