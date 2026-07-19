//! Run-command action handler — the shell IO boundary for `.px` procedures.
//!
//! This module wires the real [`ShellExecutor`](crate::shell_executor::ShellExecutor)
//! into the `.px` action-dispatch chain as the `run_command` action, so a
//! procedure step like `run_command {command: "gh pr list --json ..."}` reaches
//! a real subprocess instead of falling through to the tool dispatcher (which
//! has no `run_command` tool registered and would return an error string).
//!
//! # Why this exists (2026-07-08, TASK-2026-07-08-briefing-px STEP 0)
//!
//! A probe of the assembled runtime proved `run_command` was **NOT wired**:
//! `ShellExecutor` was instantiated only in its own unit tests, and the sole
//! non-test `ToolDispatcher` ([`SpineProcedureDispatcher`](crate::spine::dispatcher))
//! routes only to `TaskRegistryTool` built-ins and the procedure registry — no
//! `run_command` branch. This handler closes that gap with a **real** impl
//! (C-NOSTUB-001), mirroring the [`WorktaskActionHandler`](crate::spine::worktask_actions)
//! pattern: a small allowlist gate ([`is_run_command_action`]) plus a handler
//! that owns the executor. It is added as an arm of
//! [`CompositeActionHandler`](crate::spine::actions::CompositeActionHandler),
//! which is constructed in-repo in `build_reactive_runtime`, so the wiring lands
//! in the runtime this repo controls.
//!
//! # Safety (C-NOSTUB-001, C-PLURES-004)
//!
//! - Every call runs a **real** subprocess via `ShellExecutor::exec`. No canned
//!   values, no stub.
//! - Before executing, the command string is checked against the real
//!   [`ToolGovernor`](crate::tool_governance::ToolGovernor) `run_command` policy
//!   (blocked destructive patterns → refused with a structured error). This is
//!   the same governance surface documented for the `run_command` tool.
//! - The handler is the **side-effect boundary**; the branching/classification
//!   logic that decides *what* to run and *how to react to failure* stays in the
//!   `.px` procedure (C-PLURES-004). On failure (spawn error, timeout, or
//!   non-zero exit) it returns a structured `{available:false, ...}` object so
//!   the procedure's failure branch can note the gap and continue — it never
//!   panics and never mutates a file.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{debug, warn};

use crate::px_adapter::AsyncActionHandler;
use crate::shell_executor::{ExecRequest, ShellExecutor};
use crate::tool_governance::{GovernanceVerdict, ToolGovernor};
use pares_radix_praxis::px::executor::ExecutionError;

/// Actions handled by the run-command handler.
pub const RUN_COMMAND_ACTIONS: &[&str] = &["run_command"];

/// Check whether an action name is handled by the run-command handler.
#[must_use]
pub fn is_run_command_action(action: &str) -> bool {
    RUN_COMMAND_ACTIONS.contains(&action)
}

/// The `.px` `run_command` action boundary: a real [`ShellExecutor`] governed by
/// the real [`ToolGovernor`] `run_command` policy.
pub struct RunCommandActionHandler {
    executor: Arc<ShellExecutor>,
    governor: ToolGovernor,
}

impl Default for RunCommandActionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl RunCommandActionHandler {
    /// Create a handler with a fresh executor and the default governance policies.
    #[must_use]
    pub fn new() -> Self {
        Self {
            executor: Arc::new(ShellExecutor::new()),
            governor: ToolGovernor::with_defaults(),
        }
    }

    /// Create a handler over an existing (shared) executor — lets a host reuse a
    /// single session-tracking executor across the runtime if desired.
    #[must_use]
    pub fn with_executor(executor: Arc<ShellExecutor>) -> Self {
        Self {
            executor,
            governor: ToolGovernor::with_defaults(),
        }
    }

    /// Execute `run_command`.
    ///
    /// Params are deserialized into [`ExecRequest`] (`command` required;
    /// optional `workdir`, `env`, `timeout_secs`). The command is governance-
    /// checked before running. Returns a JSON object the `.px` layer can branch
    /// on:
    /// `{available, exit_code, stdout, stderr, timed_out}` on a completed run, or
    /// `{available:false, error}` when the command was refused, could not be
    /// deserialized, or was backgrounded (unsupported in this synchronous
    /// gather boundary).
    async fn run_command(&self, params: &Value) -> Result<Value, ExecutionError> {
        // Deserialize the request. A missing/blank `command` is a real usage
        // error → surface it (not a silent success).
        let req: ExecRequest = serde_json::from_value(params.clone()).map_err(|e| {
            ExecutionError::ActionFailed {
                action: "run_command".to_string(),
                message: format!("invalid run_command params: {e}"),
            }
        })?;

        if req.command.trim().is_empty() {
            return Err(ExecutionError::ActionFailed {
                action: "run_command".to_string(),
                message: "run_command requires a non-empty `command`".to_string(),
            });
        }

        // This gather boundary is synchronous by design: a `.px` gather branch
        // needs the parsed output NOW. Reject background/yield modes rather than
        // silently detaching (which would return no data to classify).
        if req.background || req.yield_ms.is_some() {
            return Ok(json!({
                "available": false,
                "error": "run_command background/yield modes are not supported in the gather boundary; run foreground",
            }));
        }

        // Real governance check against the real `run_command` policy.
        match self.governor.check("run_command", &req.command) {
            GovernanceVerdict::Blocked { pattern } => {
                warn!(command = %req.command, pattern = %pattern, "run_command blocked by governance");
                return Ok(json!({
                    "available": false,
                    "error": format!("run_command blocked by governance (matched: {pattern})"),
                }));
            }
            GovernanceVerdict::AllowWithApprovalWarning => {
                debug!(command = %req.command, "run_command allowed with approval warning");
            }
            GovernanceVerdict::Allow => {}
        }

        debug!(command = %req.command, "run_command executing (foreground)");

        // Real subprocess execution.
        let result = self.executor.exec(req).await;

        // A completed run — including a non-zero exit — is a *successful gather*
        // that returns data; the `.px` layer decides urgency/availability from
        // exit_code. A timeout is surfaced as available:false so the failure
        // branch notes the gap.
        if result.timed_out {
            return Ok(json!({
                "available": false,
                "error": "run_command timed out",
                "stdout": result.stdout,
                "stderr": result.stderr,
            }));
        }

        Ok(json!({
            "available": true,
            "exit_code": result.exit_code,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "timed_out": false,
        }))
    }
}

#[async_trait]
impl AsyncActionHandler for RunCommandActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "run_command" => self.run_command(params).await,
            other => Err(ExecutionError::UnknownAction(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate recognizes exactly `run_command`.
    #[test]
    fn gate_matches_run_command_only() {
        assert!(is_run_command_action("run_command"));
        assert!(!is_run_command_action("write_state"));
        assert!(!is_run_command_action("git_push_branch"));
    }

    /// A real echo runs and its stdout is captured (build-the-binary-run-it:
    /// this exercises the real ShellExecutor, not a mock).
    #[tokio::test]
    async fn runs_real_command_and_captures_stdout() {
        let handler = RunCommandActionHandler::new();
        // `echo` exists on both cmd.exe (Windows) and sh (unix).
        let out = handler
            .call("run_command", &json!({"command": "echo px_gather_ok"}))
            .await
            .expect("run_command should succeed");
        assert_eq!(out["available"], json!(true));
        assert_eq!(out["exit_code"], json!(0));
        assert!(
            out["stdout"].as_str().unwrap().contains("px_gather_ok"),
            "stdout was: {:?}",
            out["stdout"]
        );
    }

    /// A destructive command is refused by governance and returns a structured
    /// failure the `.px` failure branch can inspect — NOT executed.
    #[tokio::test]
    async fn blocks_destructive_command_via_governance() {
        let handler = RunCommandActionHandler::new();
        let out = handler
            .call("run_command", &json!({"command": "rm -rf /"}))
            .await
            .expect("blocked command returns structured failure, not an Err");
        assert_eq!(out["available"], json!(false));
        assert!(
            out["error"].as_str().unwrap().contains("blocked by governance"),
            "error was: {:?}",
            out["error"]
        );
    }

    /// A missing command is a real usage error (not a silent success).
    #[tokio::test]
    async fn empty_command_is_an_error() {
        let handler = RunCommandActionHandler::new();
        let err = handler
            .call("run_command", &json!({"command": "   "}))
            .await
            .expect_err("blank command must error");
        match err {
            ExecutionError::ActionFailed { action, .. } => assert_eq!(action, "run_command"),
            other => panic!("expected ActionFailed, got {other:?}"),
        }
    }

    /// A non-zero exit is still a completed gather (available:true) — the `.px`
    /// layer inspects exit_code; the boundary does not conflate exit!=0 with
    /// unavailability.
    #[tokio::test]
    async fn nonzero_exit_is_available_with_code() {
        let handler = RunCommandActionHandler::new();
        // `exit 3` is a valid non-zero exit under both `sh -c` (Unix) and
        // `cmd /C` (Windows), so no per-OS branch is needed here.
        let cmd = "exit 3";
        let out = handler
            .call("run_command", &json!({"command": cmd}))
            .await
            .expect("run_command should complete");
        assert_eq!(out["available"], json!(true));
        assert_eq!(out["exit_code"], json!(3));
    }
}
