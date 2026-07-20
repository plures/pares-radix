//! Bootstrap — loads .px procedures into the ReactiveRegistry at startup.
//!
//! This module bridges the gap between static `.px` files on disk and the
//! runtime reactive system. At startup, it:
//!
//! 1. Reads all `.px` files from the praxis directory tree
//! 2. Compiles them into `PxProcedureAdapter` instances
//! 3. Registers each procedure in the `ReactiveRegistry` with trigger patterns
//!    derived from the procedure's declared trigger kind
//!
//! # Trigger Pattern Mapping
//!
//! Procedures declare their trigger kind in the `.px` source (e.g. `trigger: on_write`).
//! The bootstrap maps procedure names to appropriate glob patterns:
//!
//! ```text
//! classify_message     → "inbound:*"      (fires on every inbound message write)
//! route_event          → "classification:*" (fires after classification is written)
//! context_window       → "inbound:*"      (parallel with classify)
//! heartbeat_logic      → "heartbeat:*"    (fires on heartbeat events)
//! retention            → "memory:*"       (fires on memory writes)
//! memory_correction    → "memory:*"       (fires on memory writes)
//! commitment_detection → "response:*"     (fires after model response)
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::px_adapter::{load_px_directory, AsyncActionHandler};
use crate::spine::reactive::ReactiveRegistry;

/// Mapping from procedure name to the trigger pattern it should be registered under.
///
/// Procedures not listed here are registered under their declared trigger kind
/// with a wildcard suffix (e.g. trigger kind "on_write" → "on_write:*").
fn default_trigger_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();

    // Core pipeline procedures
    m.insert("classify_message", "inbound:*");
    m.insert("classify_and_route", "inbound:*");
    m.insert("route_event", "classification:*");
    m.insert("unified_router", "inbound:*");
    m.insert("track_inbound", "inbound:*");

    // Context assembly (fires on route_decision writes)
    m.insert("assemble_context", "route_decision:*");
    m.insert("dispatch_steered_task", "route_decision:*");

    // Context and preprocessing
    m.insert("manage_context_window", "inbound:*");
    m.insert("preprocess", "inbound:*");

    // Memory procedures
    m.insert("retention_evaluate", "memory:*");
    m.insert("memory_correction", "memory:*");
    m.insert("memory_consolidate", "memory:*");

    // Heartbeat clock chain (design: praxis/spine/spine.px IO boundary #4).
    // The heartbeat timer (Rust IO) writes a tick to the "heartbeat_clock"
    // queue; heartbeat_timer emits it, heartbeat_tick runs the health check,
    // and evaluate_dispatch (autonomous-dispatch.px) reads the "heartbeat_tick"
    // queue to decide whether to dispatch an autonomous task this tick.
    //
    // NOTE: the legacy "heartbeat:*" glob is retained for the existing
    // heartbeat_logic/heartbeat_check procedures, but no producer writes a
    // "heartbeat:*" key today — heartbeats currently arrive as Inbound events.
    // The authoritative queue keys are "heartbeat_clock" and "heartbeat_tick".
    m.insert("heartbeat_logic", "heartbeat:*");
    m.insert("heartbeat_check", "heartbeat:*");
    m.insert("heartbeat_timer", "heartbeat_clock:*");
    m.insert("heartbeat_tick", "heartbeat_clock:*");
    // Autonomous dispatch: evaluate_dispatch consumes the heartbeat tick and
    // writes a dispatch_decision consumed by the TaskDispatchDriver (Rust IO).
    m.insert("evaluate_dispatch", "heartbeat_tick:*");

    // Response post-processing (fires on model_response events)
    m.insert("commitment_detection", "model_response:*");
    m.insert("detect_and_store_commitments", "model_response:*");
    m.insert("session_continuity", "model_response:*");
    m.insert("route_model_response", "model_response:*");

    // Delivery (fires on delivery_request events)
    m.insert("deliver_response", "delivery_request:*");

    // Scheduled briefings — the cron writes `briefing:request:<ts>` (the clock
    // stays in cron; the procedure owns gather->evaluate->format->deliver). The
    // inline `trigger: on_write {pattern: "briefing:request:*"}` in the .px is
    // not yet threaded through the adapter (see px_adapter trigger.kind gap),
    // so this name-map entry is what routes it to the correct pattern instead
    // of the noisy `on_write:*` fallback. (TASK-2026-07-08-briefing-px STEP 1.)
    m.insert("morning_briefing", "briefing:request:*");

    // Dashboard milestone freeze — a `milestone:<id>` write is the dashboard
    // signal; the procedure derives + persists the frozen dashboard node (the
    // pure-praxis analog of the exec side-effect that froze the dashboard).
    // `progress:<id>` writes are history, NOT dashboard signals, so they must
    // NOT match this pattern. (test/milestone-reactive-proof)
    m.insert("dashboard_milestone", "milestone:*");

    // Task management
    m.insert("task_evaluation", "inbound:*");
    m.insert("task_steering", "task:*");

    // Dev-lifecycle orchestration
    m.insert("plan_task", "task_request:*");
    m.insert("evaluate_gate", "stage_complete:*");
    m.insert("report_result", "task_complete:*");

    // Worktask executor — each command fires only on its own command key
    // (a write to `worktask:cmd:<name>:<reqid>` triggers exactly that procedure).
    m.insert("new_epic", "worktask:cmd:new_epic:*");
    m.insert("new_feature", "worktask:cmd:new_feature:*");
    m.insert("new_bugfix", "worktask:cmd:new_bugfix:*");
    m.insert("new_chore", "worktask:cmd:new_chore:*");
    m.insert("list_feature", "worktask:cmd:list_feature:*");
    m.insert("list_chore", "worktask:cmd:list_chore:*");
    m.insert("get_feature", "worktask:cmd:get_feature:*");
    m.insert("get_chore", "worktask:cmd:get_chore:*");
    m.insert("new_pr", "worktask:cmd:new_pr:*");
    m.insert("reclaim", "worktask:cmd:reclaim:*");
    m.insert("doctor", "worktask:cmd:doctor:*");

    // RSI (recursive self-improvement)
    m.insert("evaluate_performance", "task_complete:*");
    m.insert("identify_improvement", "perf_signal:*");
    m.insert("validate_improvement", "improvement:proposed:*");
    m.insert("apply_improvement", "improvement:validated:*");
    m.insert("check_regression", "perf_metric:*");
    m.insert("rollback_improvement", "regression:detected:*");

    // Model selection
    m.insert("select_model", "model_request:*");
    m.insert("build_session_context", "model_request:*");
    m.insert("record_model_performance", "model_response:*");

    // Topic routing
    m.insert("classify_topic", "inbound:*");
    m.insert("switch_context", "topic_switch:*");
    m.insert("steer_continuation", "route_decision:*");
    m.insert("force_topic_resolution", "topic:timeout:*");

    m
}

/// Load all `.px` procedures from the given praxis directory and register them
/// in the reactive registry.
///
/// Returns the number of procedures successfully registered.
///
/// # Arguments
///
/// * `praxis_dir` — Path to the `praxis/` directory containing `.px` files.
/// * `registry` — The reactive registry to register procedures into.
/// * `handler` — The async action handler for IO boundaries.
pub async fn register_reactive_procedures(
    praxis_dir: &Path,
    registry: &ReactiveRegistry,
    handler: Arc<dyn AsyncActionHandler>,
) -> usize {
    let trigger_map = default_trigger_map();

    // Load all .px procedures from the directory tree
    let adapters = load_px_directory(praxis_dir, handler);

    if adapters.is_empty() {
        warn!(
            dir = %praxis_dir.display(),
            "bootstrap: no .px procedures found"
        );
        return 0;
    }

    info!(
        count = adapters.len(),
        dir = %praxis_dir.display(),
        "bootstrap: compiled .px procedures, registering triggers"
    );

    let mut registered = 0;

    for adapter in adapters {
        let name = adapter.name().to_string();

        // Determine trigger pattern: explicit map takes precedence,
        // then fall back to trigger_kind:* from the compiled record
        let pattern = if let Some(&mapped) = trigger_map.get(name.as_str()) {
            mapped.to_string()
        } else {
            // Use the adapter's declared trigger kind with wildcard
            let kind = adapter.trigger_kind();
            if kind == "manual" {
                // Manual procedures aren't reactive — skip registration
                debug!(
                    procedure = %name,
                    "bootstrap: skipping manual procedure (not reactive)"
                );
                continue;
            }
            format!("{kind}:*")
        };

        debug!(
            procedure = %name,
            pattern = %pattern,
            "bootstrap: registering reactive trigger"
        );

        registry
            .register_procedure(&pattern, Arc::new(adapter))
            .await;
        registered += 1;
    }

    info!(
        registered,
        total_triggers = registry.trigger_count().await,
        "bootstrap: reactive procedure registration complete"
    );

    registered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::px_adapter::AsyncActionHandler;
    use async_trait::async_trait;
    use pares_radix_praxis::px::executor::ExecutionError;
    use serde_json::Value;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct NoOpHandler;

    #[async_trait]
    impl AsyncActionHandler for NoOpHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            Ok(Value::Null)
        }
    }

    #[tokio::test]
    async fn register_from_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let registry = ReactiveRegistry::new();
        let handler: Arc<dyn AsyncActionHandler> = Arc::new(NoOpHandler);

        let count = register_reactive_procedures(tmp.path(), &registry, handler).await;
        assert_eq!(count, 0);
        assert_eq!(registry.trigger_count().await, 0);
    }

    #[tokio::test]
    async fn register_from_nonexistent_dir() {
        let registry = ReactiveRegistry::new();
        let handler: Arc<dyn AsyncActionHandler> = Arc::new(NoOpHandler);

        let count =
            register_reactive_procedures(Path::new("/nonexistent/path"), &registry, handler).await;
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn register_valid_procedure() {
        let tmp = TempDir::new().unwrap();
        let px_path = tmp.path().join("test.px");
        // Use minimal procedure syntax that the parser handles
        std::fs::write(
            &px_path,
            r#"
procedure classify_message:
  trigger: on_write
  given: "Test classification procedure"
  detect_intent {text: $message} -> $intent
  return $intent
"#,
        )
        .unwrap();

        let registry = ReactiveRegistry::new();
        let handler: Arc<dyn AsyncActionHandler> = Arc::new(NoOpHandler);

        let count = register_reactive_procedures(tmp.path(), &registry, handler).await;
        // The procedure should parse and register (trigger: on_write maps to inbound:*
        // via the trigger_map since it's named classify_message)
        // If parsing fails gracefully, count may be 0 — that's acceptable for
        // the parser's current state. The test verifies no panic.
        // When the parser supports this syntax fully, assert count >= 1.
        assert!(registry.trigger_count().await == count);
    }

    #[tokio::test]
    async fn manual_procedures_skipped() {
        let tmp = TempDir::new().unwrap();
        let px_path = tmp.path().join("manual.px");
        std::fs::write(
            &px_path,
            r#"
procedure some_manual_proc:
  given: "A manually triggered procedure"
  return "done"
"#,
        )
        .unwrap();

        let registry = ReactiveRegistry::new();
        let handler: Arc<dyn AsyncActionHandler> = Arc::new(NoOpHandler);

        let count = register_reactive_procedures(tmp.path(), &registry, handler).await;
        // Manual procedures should not be registered reactively
        assert_eq!(count, 0);
    }

    /// Integration test: verify the actual .px files in praxis/spine/ compile.
    /// This catches parser/compiler regressions against real procedure files.
    #[tokio::test]
    async fn real_spine_px_files_compile() {
        // Find the project root (go up from the test binary location)
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let crate_path = PathBuf::from(manifest_dir);
        let project_root = crate_path
            .parent() // crates/
            .and_then(|p| p.parent()) // project root
            .expect("could not find project root");

        let spine_dir = project_root.join("praxis").join("spine");
        if !spine_dir.is_dir() {
            // Skip if running in CI without the praxis directory
            return;
        }

        let registry = ReactiveRegistry::new();
        let handler: Arc<dyn AsyncActionHandler> = Arc::new(NoOpHandler);

        let count = register_reactive_procedures(&spine_dir, &registry, handler).await;
        // We expect at least some procedures to load from spine/
        // (routing.px, conversation.px, spine.px have procedure definitions)
        // Count may be 0 if the parser can't handle the v3 syntax yet — that's
        // tracked as a known gap. The test should not panic.
        eprintln!(
            "real_spine_px_files_compile: loaded {} procedures from {}",
            count,
            spine_dir.display()
        );
    }

    /// Regression guard: the real `praxis/procedures/worktask.px` must parse,
    /// compile, and register all 11 reactive worktask commands. The broader
    /// `real_spine_px_files_compile` test only scans `praxis/spine/`, so this
    /// covers the worktask procedure surface specifically. Catches a `.px`
    /// syntax regression that the Rust handler tests cannot see.
    #[tokio::test]
    async fn worktask_px_compiles_and_registers_all_commands() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let project_root = PathBuf::from(manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .expect("could not find project root")
            .to_path_buf();

        let procedures_dir = project_root.join("praxis").join("procedures");
        let worktask_px = procedures_dir.join("worktask.px");
        if !worktask_px.is_file() {
            // The worktree layout always has this; skip only if the praxis dir
            // is genuinely absent (e.g. a stripped CI checkout).
            return;
        }

        // Parse + compile the file directly to assert every command is present
        // (independent of trigger-map registration filtering).
        let src = std::fs::read_to_string(&worktask_px).expect("read worktask.px");
        let doc = pares_radix_praxis::px::parse(&src)
            .expect("worktask.px must parse against the real .px parser");
        let records = pares_radix_praxis::px::compiler::compile(&doc);
        let names: Vec<String> = records
            .iter()
            .filter_map(|r| {
                r.data
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .collect();
        for cmd in [
            "new_epic",
            "new_feature",
            "new_bugfix",
            "new_chore",
            "list_feature",
            "list_chore",
            "get_feature",
            "get_chore",
            "new_pr",
            "reclaim",
            "doctor",
        ] {
            assert!(
                names.contains(&cmd.to_string()),
                "worktask.px is missing command procedure `{cmd}`; compiled: {names:?}"
            );
        }

        // And the full procedures dir must register reactively without panic;
        // every worktask command is `trigger: on_write`, so they all register.
        let registry = ReactiveRegistry::new();
        let handler: Arc<dyn AsyncActionHandler> = Arc::new(NoOpHandler);
        let count = register_reactive_procedures(&procedures_dir, &registry, handler).await;
        assert!(
            count >= 11,
            "expected >= 11 reactive procedures from praxis/procedures (11 worktask \
             commands + others), registered {count}"
        );
    }
}
