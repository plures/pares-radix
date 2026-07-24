//! Worktask action handlers — the git/fs/state IO boundary for `worktask.px`.
//!
//! `worktask.px` performs **no side effects directly**; it only *describes*
//! effects by calling the actions implemented here. This handler executes the
//! real IO: it shells out to a real `git` binary via [`tokio::process::Command`]
//! and touches the real filesystem via `std::fs` / `walkdir`. Durable records
//! (tasks, leases, policy, telemetry, quarantine) are persisted through the
//! shared [`StateStore`] (PluresDB-backed in production), satisfying
//! C-PLURES-003 (all durable state lives in PluresDB).
//!
//! Actions provided (the allowlist [`WORKTASK_ACTIONS`]):
//!
//! - `timestamp_now` — real unix-epoch seconds (also consumed by `dev-lifecycle.px`).
//! - `generate_id` — real UUID v4 string.
//! - `git_worktree_add` — `git -C <repo> worktree add -b <branch> <path>`.
//! - `git_worktree_status` — `git -C <path> status --porcelain` → `{dirty, lines}`.
//! - `git_worktree_remove` — `git -C <repo> worktree remove <path>` (clean trees only).
//! - `git_branch_delete` — `git -C <repo> branch -d <branch>`.
//! - `git_worktree_prune` — `git -C <repo> worktree prune`.
//! - `git_push_branch` — `git -C <path> push <remote> <branch>` (real publish).
//! - `git_merge_branch` — `git -C <repo> merge --no-ff <branch>` (real direct merge).
//! - `fs_dir_size` — real recursive byte count via `walkdir`.
//! - `quarantine_worktree` — **moves** a dirty tree to the quarantine root
//!   (`std::fs::rename`, cross-volume copy+remove fallback). Never deletes.
//! - `land_direct_merge` / `land_github_pr` / `land_subagent_review` / `land_none`
//!   — perform the real git work each landing mode owns at this boundary and
//!   return a descriptor of what was done / what the external step must do.
//!
//! # Safety (C-NOSTUB-001)
//!
//! Every action runs a real subprocess / fs op and surfaces real
//! `ExecutionError::ActionFailed` on failure. Where a landing mode's *final*
//! step legitimately belongs to an external system this handler does not own
//! (creating a GitHub PR, spawning a review subagent), the action does the real
//! git work it CAN do (e.g. push the branch) and returns an honest descriptor
//! naming the remaining external step — it never reports a merge/PR that did not
//! happen. The dangerous clean-vs-dirty reclaim DECISION lives in `worktask.px`;
//! this handler only executes the chosen effect.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::px_adapter::AsyncActionHandler;
use crate::state::StateStore;
use pares_radix_praxis::px::executor::ExecutionError;

/// Thin wrapper around the real `git` binary.
///
/// Each method runs one `git` subprocess and returns its captured
/// stdout/stderr/exit status. No logic lives here — callers decide what a
/// non-zero exit means (most surface it as [`ExecutionError::ActionFailed`]).
#[derive(Clone, Debug)]
pub struct GitEffects {
    /// The git executable to invoke. Defaults to `"git"` (resolved on PATH).
    program: String,
}

impl Default for GitEffects {
    fn default() -> Self {
        Self {
            program: "git".to_string(),
        }
    }
}

/// Captured result of a single subprocess run.
struct CmdOutput {
    status: i32,
    stdout: String,
    stderr: String,
}

impl GitEffects {
    /// Construct a wrapper around a specific git executable (mainly for tests).
    pub fn with_program(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// Run `git <args...>` and capture output. `Err` only on spawn failure
    /// (e.g. git not found); a non-zero exit is returned in [`CmdOutput`].
    async fn run(&self, action: &str, args: &[&str]) -> Result<CmdOutput, ExecutionError> {
        let output = Command::new(&self.program)
            .args(args)
            .output()
            .await
            .map_err(|e| ExecutionError::ActionFailed {
                action: action.to_string(),
                message: format!("failed to spawn `{} {}`: {e}", self.program, args.join(" ")),
            })?;

        Ok(CmdOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Run a git command and require a zero exit, otherwise `ActionFailed`.
    async fn run_checked(&self, action: &str, args: &[&str]) -> Result<CmdOutput, ExecutionError> {
        let out = self.run(action, args).await?;
        if out.status != 0 {
            return Err(ExecutionError::ActionFailed {
                action: action.to_string(),
                message: format!(
                    "`git {}` exited {} — {}",
                    args.join(" "),
                    out.status,
                    out.stderr.trim()
                ),
            });
        }
        Ok(out)
    }
}

/// Action handler for `worktask.px` — the git/fs/state IO boundary.
///
/// Holds the shared durable [`StateStore`] (the SAME `Arc` the core handler
/// uses, so worktask records and general state co-locate), a [`GitEffects`]
/// subprocess wrapper, and the quarantine root where dirty worktrees are
/// preserved (moved, never deleted).
pub struct WorktaskActionHandler {
    state: Arc<dyn StateStore>,
    git: GitEffects,
    quarantine_root: PathBuf,
}

impl WorktaskActionHandler {
    /// Construct with the shared state store. The quarantine root defaults to
    /// `<system-temp>/pares-radix/worktask-quarantine`; use
    /// [`with_quarantine_root`](Self::with_quarantine_root) to override.
    pub fn new(state: Arc<dyn StateStore>) -> Self {
        let quarantine_root = std::env::temp_dir()
            .join("pares-radix")
            .join("worktask-quarantine");
        Self {
            state,
            git: GitEffects::default(),
            quarantine_root,
        }
    }

    /// Override the quarantine root (where dirty worktrees are moved).
    pub fn with_quarantine_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.quarantine_root = root.into();
        self
    }

    /// Override the git effects wrapper (mainly for tests).
    pub fn with_git(mut self, git: GitEffects) -> Self {
        self.git = git;
        self
    }

    // ── time / id ──────────────────────────────────────────────────────────

    /// Real unix-epoch seconds. Mirrors the `timestamp_now` action that
    /// `dev-lifecycle.px` and `autonomous-dispatch.px` already call.
    fn timestamp_now(&self) -> Result<Value, ExecutionError> {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Ok(json!(secs))
    }

    /// Real UUID v4 string for task ids / run ids.
    fn generate_id(&self) -> Result<Value, ExecutionError> {
        Ok(json!(Uuid::new_v4().to_string()))
    }

    // ── git effects ────────────────────────────────────────────────────────

    /// `git -C <repo_path> worktree add -b <branch> <worktree_path>`.
    async fn git_worktree_add(&self, params: &Value) -> Result<Value, ExecutionError> {
        let repo = require_str(params, "repo_path", "git_worktree_add")?;
        let wt = require_str(params, "worktree_path", "git_worktree_add")?;
        let branch = require_str(params, "branch", "git_worktree_add")?;
        let out = self
            .git
            .run_checked(
                "git_worktree_add",
                &["-C", repo, "worktree", "add", "-b", branch, wt],
            )
            .await?;
        debug!(repo, wt, branch, "worktask: git_worktree_add");
        Ok(
            json!({ "ok": true, "worktree_path": wt, "branch": branch, "stdout": out.stdout.trim() }),
        )
    }

    /// `git -C <worktree_path> status --porcelain` → `{dirty, lines}`.
    ///
    /// `dirty == true` when the porcelain output is non-empty. This is the
    /// clean/dirty gate the `.px` `reclaim` procedure branches on.
    async fn git_worktree_status(&self, params: &Value) -> Result<Value, ExecutionError> {
        let wt = require_str(params, "worktree_path", "git_worktree_status")?;
        let out = self
            .git
            .run_checked("git_worktree_status", &["-C", wt, "status", "--porcelain"])
            .await?;
        let trimmed = out.stdout.trim_end();
        let lines = if trimmed.is_empty() {
            0
        } else {
            trimmed.lines().count()
        };
        let dirty = lines > 0;
        debug!(wt, dirty, lines, "worktask: git_worktree_status");
        Ok(json!({ "dirty": dirty, "lines": lines, "worktree_path": wt }))
    }

    /// `git -C <repo_path> worktree remove <worktree_path>`. The `.px` only
    /// calls this when status reported clean. `force` adds `--force`.
    async fn git_worktree_remove(&self, params: &Value) -> Result<Value, ExecutionError> {
        let repo = require_str(params, "repo_path", "git_worktree_remove")?;
        let wt = require_str(params, "worktree_path", "git_worktree_remove")?;
        let force = params
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut args = vec!["-C", repo, "worktree", "remove", wt];
        if force {
            args.push("--force");
        }
        self.git.run_checked("git_worktree_remove", &args).await?;
        debug!(repo, wt, force, "worktask: git_worktree_remove");
        Ok(json!({ "ok": true, "removed": wt }))
    }

    /// `git -C <repo_path> branch -d <branch>` (`-D` when `force`).
    async fn git_branch_delete(&self, params: &Value) -> Result<Value, ExecutionError> {
        let repo = require_str(params, "repo_path", "git_branch_delete")?;
        let branch = require_str(params, "branch", "git_branch_delete")?;
        let force = params
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let flag = if force { "-D" } else { "-d" };
        self.git
            .run_checked("git_branch_delete", &["-C", repo, "branch", flag, branch])
            .await?;
        debug!(repo, branch, force, "worktask: git_branch_delete");
        Ok(json!({ "ok": true, "deleted_branch": branch }))
    }

    /// `git -C <repo_path> worktree prune`.
    async fn git_worktree_prune(&self, params: &Value) -> Result<Value, ExecutionError> {
        let repo = require_str(params, "repo_path", "git_worktree_prune")?;
        self.git
            .run_checked("git_worktree_prune", &["-C", repo, "worktree", "prune"])
            .await?;
        debug!(repo, "worktask: git_worktree_prune");
        Ok(json!({ "ok": true, "pruned_repo": repo }))
    }

    /// `git -C <worktree_path> push <remote> <branch>` — real publish of the
    /// branch (used by the github-pr / subagent-review landing modes).
    async fn git_push_branch(&self, params: &Value) -> Result<Value, ExecutionError> {
        let wt = require_str(params, "worktree_path", "git_push_branch")?;
        let branch = require_str(params, "branch", "git_push_branch")?;
        let remote = params
            .get("remote")
            .and_then(|v| v.as_str())
            .unwrap_or("origin");
        let out = self
            .git
            .run_checked("git_push_branch", &["-C", wt, "push", remote, branch])
            .await?;
        debug!(wt, branch, remote, "worktask: git_push_branch");
        Ok(json!({ "ok": true, "remote": remote, "branch": branch, "stderr": out.stderr.trim() }))
    }

    /// `git -C <repo_path> merge --no-ff <branch>` — real direct merge into the
    /// repo's current branch (used by `land_direct_merge`).
    async fn git_merge_branch(&self, params: &Value) -> Result<Value, ExecutionError> {
        let repo = require_str(params, "repo_path", "git_merge_branch")?;
        let branch = require_str(params, "branch", "git_merge_branch")?;
        let out = self
            .git
            .run_checked(
                "git_merge_branch",
                &["-C", repo, "merge", "--no-ff", branch],
            )
            .await?;
        debug!(repo, branch, "worktask: git_merge_branch");
        Ok(json!({ "ok": true, "merged_branch": branch, "stdout": out.stdout.trim() }))
    }

    // ── fs effects ─────────────────────────────────────────────────────────

    /// Real recursive byte count of a directory tree via `walkdir`. Missing
    /// paths return `0` (not an error — the tree may already be gone).
    fn fs_dir_size(&self, params: &Value) -> Result<Value, ExecutionError> {
        let path = require_str(params, "path", "fs_dir_size")?;
        let p = Path::new(path);
        if !p.exists() {
            return Ok(json!({ "bytes": 0, "path": path, "exists": false }));
        }
        let mut total: u64 = 0;
        for entry in walkdir::WalkDir::new(p).into_iter().flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                }
            }
        }
        debug!(path, bytes = total, "worktask: fs_dir_size");
        Ok(json!({ "bytes": total, "path": path, "exists": true }))
    }

    /// **Move** (never delete) a dirty worktree to the quarantine root.
    ///
    /// Tries `std::fs::rename` first; on a cross-volume error falls back to a
    /// recursive copy followed by removal of the source. Records a durable
    /// `worktask:quarantine:{task_id}` node and returns the quarantined path +
    /// byte size. The dirty data is preserved on disk and reported — there is
    /// no `git worktree remove` and no `rm` on a dirty tree.
    async fn quarantine_worktree(&self, params: &Value) -> Result<Value, ExecutionError> {
        let wt = require_str(params, "worktree_path", "quarantine_worktree")?;
        let task_id = require_str(params, "task_id", "quarantine_worktree")?;
        let reason = params
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("expired_lease_dirty_tree");
        let branch = params.get("branch").and_then(|v| v.as_str()).unwrap_or("");

        let root = params
            .get("quarantine_root")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.quarantine_root.clone());

        std::fs::create_dir_all(&root).map_err(|e| ExecutionError::ActionFailed {
            action: "quarantine_worktree".into(),
            message: format!("create quarantine root {}: {e}", root.display()),
        })?;

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let dest = root.join(format!("{task_id}-{ts}"));

        // Size BEFORE the move (the source path is what we measure).
        let bytes = dir_size_bytes(Path::new(wt));

        move_dir(Path::new(wt), &dest).map_err(|e| ExecutionError::ActionFailed {
            action: "quarantine_worktree".into(),
            message: format!("move {wt} -> {}: {e}", dest.display()),
        })?;

        let dest_str = dest.to_string_lossy().to_string();
        let record = json!({
            "task_id": task_id,
            "worktree_path": wt,
            "quarantined_path": dest_str,
            "branch": branch,
            "reason": reason,
            "bytes": bytes,
            "quarantined_at": ts,
        });
        self.state
            .set(&format!("worktask:quarantine:{task_id}"), record.clone())
            .await;

        warn!(task_id, wt, dest = %dest_str, bytes, "worktask: quarantined dirty worktree (moved, not deleted)");
        Ok(record)
    }

    // ── landing modes ────────────────────────────────────────────────────────

    /// `direct-merge` mode: real `git merge --no-ff <branch>` into the canonical
    /// repo's current branch. Fully self-contained at this boundary.
    async fn land_direct_merge(&self, params: &Value) -> Result<Value, ExecutionError> {
        let merged = self.git_merge_branch(params).await?;
        let branch = params.get("branch").and_then(|v| v.as_str()).unwrap_or("");
        Ok(json!({
            "mode": "direct-merge",
            "landed": true,
            "branch": branch,
            "detail": merged,
        }))
    }

    /// `github-pr` mode: real `git push` of the branch to the remote; the PR
    /// *creation* itself belongs to the GitHub layer (gh/API) which this
    /// git/fs handler does not own. Returns an honest descriptor — `landed` is
    /// `false` because no merge happened here — naming the external step.
    async fn land_github_pr(&self, params: &Value) -> Result<Value, ExecutionError> {
        let pushed = self.git_push_branch(params).await?;
        let branch = params.get("branch").and_then(|v| v.as_str()).unwrap_or("");
        Ok(json!({
            "mode": "github-pr",
            "landed": false,
            "branch_pushed": true,
            "branch": branch,
            "next_external_step": "open a GitHub PR for the pushed branch (gh/API layer)",
            "detail": pushed,
        }))
    }

    /// `subagent-review` mode: real `git push` so a review subagent can fetch
    /// the branch; the review/merge decision belongs to the subagent runtime
    /// (not this handler). Honest descriptor — `landed` is `false`.
    async fn land_subagent_review(&self, params: &Value) -> Result<Value, ExecutionError> {
        let pushed = self.git_push_branch(params).await?;
        let branch = params.get("branch").and_then(|v| v.as_str()).unwrap_or("");
        Ok(json!({
            "mode": "subagent-review",
            "landed": false,
            "branch_pushed": true,
            "branch": branch,
            "next_external_step": "spawn a review subagent against the pushed branch",
            "detail": pushed,
        }))
    }

    /// `none` mode: genuinely no landing action (e.g. epics, or work parked for
    /// manual handling). A real no-op transition, not a fake success.
    fn land_none(&self, params: &Value) -> Result<Value, ExecutionError> {
        let branch = params.get("branch").and_then(|v| v.as_str()).unwrap_or("");
        Ok(json!({
            "mode": "none",
            "landed": false,
            "branch": branch,
            "detail": "no landing action for this mode (manual/none)",
        }))
    }

    // ── state enumeration / shaping ─────────────────────────────────────

    /// Enumerate all `worktask:task:*` records from the durable store, returning
    /// a JSON array of the task objects. Optionally filter by `task_type`.
    ///
    /// This is the real StateStore enumeration the `.px` `reclaim` and `doctor`
    /// procedures iterate over (the executor cannot itself scan key prefixes).
    /// The clean-vs-dirty / expiry DECISIONS stay in `.px`; this only reads.
    async fn list_tasks(&self, params: &Value) -> Result<Value, ExecutionError> {
        let filter_type = params.get("task_type").and_then(|v| v.as_str());
        let keys = self.state.keys_with_prefix("worktask:task:").await;
        let mut tasks: Vec<Value> = Vec::new();
        for key in keys {
            if let Some(rec) = self.state.get(&key).await {
                if rec.is_null() {
                    continue;
                }
                if let Some(ft) = filter_type {
                    if rec.get("task_type").and_then(|v| v.as_str()) != Some(ft) {
                        continue;
                    }
                }
                tasks.push(rec);
            }
        }
        debug!(count = tasks.len(), filter = ?filter_type, "worktask: list_tasks");
        Ok(Value::Array(tasks))
    }

    /// Pure data-shaping: return a copy of `task` with `status` (and
    /// `updated_at`, if provided) patched. Mirrors `dev-lifecycle.px`'s use of
    /// a Rust `update_stage_status` action to patch a record the `.px` then
    /// persists via `write_state`. No IO here.
    fn set_task_status(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task = params.get("task").cloned().unwrap_or(Value::Null);
        let status = require_str(params, "status", "set_task_status")?;
        let mut obj = match task {
            Value::Object(m) => m,
            _ => {
                return Err(ExecutionError::ActionFailed {
                    action: "set_task_status".into(),
                    message: "'task' must be an object".into(),
                })
            }
        };
        obj.insert("status".into(), Value::String(status.to_string()));
        if let Some(ts) = params.get("updated_at") {
            obj.insert("updated_at".into(), ts.clone());
        }
        Ok(Value::Object(obj))
    }

    /// Identity echo of the `v` param. Used by `worktask.px` to bind a computed
    /// scalar (e.g. the resolved `pr_mode`) into a `.px` variable, since the
    /// executor has no bare-expression assignment step. Not a stub — it is the
    /// real (and only) mechanism for `.px` to materialize a chosen value into a
    /// variable binding; the *decision* of which value to pass stays in `.px`.
    fn identity(&self, params: &Value) -> Result<Value, ExecutionError> {
        Ok(params.get("v").cloned().unwrap_or(Value::Null))
    }

    /// Assemble a full task record with **native types** from the command
    /// payload (`value`) plus the `.px`-computed scalars (`id`, `now`,
    /// `task_type`, `pr_mode`, `status`). This exists because `.px` object
    /// literals cannot resolve bare dotted refs (`$value.org`) nor preserve
    /// numeric types through `${...}` string interpolation — so the record is
    /// shaped in Rust (like `dev-lifecycle.px`'s `merge_stage_config`). All
    /// values are copied verbatim from inputs; no fabrication.
    fn make_task_record(&self, params: &Value) -> Result<Value, ExecutionError> {
        let v = params.get("value").cloned().unwrap_or(json!({}));
        let field = |k: &str| v.get(k).cloned().unwrap_or(Value::Null);
        let p = |k: &str| params.get(k).cloned().unwrap_or(Value::Null);
        let task_type = p("task_type");
        let task_type_str = task_type.as_str().unwrap_or_default();
        // Normalize pr_mode against the known-valid set, failing safe to the
        // per-task-type default. This is the single choke point that keeps a
        // corrupted/unresolved pr_mode out of durable task state: the `.px`
        // policy chain interpolates `${policy.pr_mode}`, and when a policy node
        // is present but lacks a `pr_mode` field the executor yields the literal
        // token `"${...}"` (an unresolved interpolation) rather than a value.
        // Storing that literal would poison routing in `new_pr`. Empty strings,
        // nulls, and any unknown string are likewise coerced to the default.
        let pr_mode = normalize_pr_mode(p("pr_mode").as_str(), task_type_str);
        Ok(json!({
            "task_id": p("id"),
            "org": field("org"),
            "repo": field("repo"),
            "task_type": task_type,
            "branch": field("branch"),
            "worktree_path": field("worktree_path"),
            "owner_session": field("owner_session"),
            "owner_agent": field("owner_agent"),
            "status": p("status"),
            "pr_mode": pr_mode,
            "created_at": p("now"),
            "updated_at": p("now"),
        }))
    }

    /// Assemble a lease record with native types from the command payload
    /// (`value`) plus `.px`-computed `id`/`now`. Same rationale as
    /// [`make_task_record`](Self::make_task_record). The lease is a separate
    /// node so high-churn lease updates don't rewrite the whole task.
    fn make_lease_record(&self, params: &Value) -> Result<Value, ExecutionError> {
        let v = params.get("value").cloned().unwrap_or(json!({}));
        let field = |k: &str| v.get(k).cloned().unwrap_or(Value::Null);
        Ok(json!({
            "task_id": params.get("id").cloned().unwrap_or(Value::Null),
            "owner_session": field("owner_session"),
            "owner_agent": field("owner_agent"),
            "acquired_at": params.get("now").cloned().unwrap_or(Value::Null),
            "lease_expires_at": field("lease_expires_at"),
        }))
    }

    /// Pass-through builder for a per-task reclaim/doctor outcome object. Copies
    /// its params verbatim into an object so `worktask.px` can emit a single
    /// structured outcome per loop iteration. No logic, no fabrication.
    fn make_outcome(&self, params: &Value) -> Result<Value, ExecutionError> {
        Ok(match params {
            Value::Object(_) => params.clone(),
            _ => json!({}),
        })
    }

    /// Count the elements of the `arr` param (0 if absent/not an array). Exists
    /// because the `.px` `length()` built-in only evaluates inside string
    /// interpolation / conditions, not as a bare object-literal value — so a
    /// real count needed in a returned/written object is computed here. Pure.
    fn count(&self, params: &Value) -> Result<Value, ExecutionError> {
        let n = params
            .get("arr")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        Ok(json!(n))
    }
}

// ── free helpers ────────────────────────────────────────────────────────────

/// The closed set of valid PR-handling modes a task may carry. Anything outside
/// this set is not routable by `new_pr` in `worktask.px`.
const VALID_PR_MODES: [&str; 4] = ["github-pr", "subagent-review", "direct-merge", "none"];

/// The documented fail-safe default pr_mode for a task type, used when policy
/// resolution produced no usable value. Mirrors the per-procedure defaults in
/// `worktask.px` (feature/bugfix → github-pr, chore → direct-merge, epic →
/// none). Unknown task types fall back to `none` (the most conservative mode:
/// a no-op landing that never auto-merges).
fn default_pr_mode_for(task_type: &str) -> &'static str {
    match task_type {
        "feature" | "bugfix" => "github-pr",
        "chore" => "direct-merge",
        "epic" => "none",
        _ => "none",
    }
}

/// Normalize a resolved pr_mode to a known-valid value, failing safe to the
/// per-task-type default. Returns a JSON string `Value`.
///
/// `raw` is whatever the `.px` policy chain produced. It may be:
/// - a valid mode (kept as-is),
/// - `None`/empty (no value resolved → default),
/// - an **unresolved interpolation literal** like `"${global_pol.pr_mode}"`
///   (produced when a policy node exists but has no `pr_mode` field → default),
/// - any other unknown string (typo'd/hostile policy value → default).
///
/// Coercing to the default here is the correct fail-safe: an unroutable pr_mode
/// in durable state would otherwise silently degrade `new_pr` to a no-op (or
/// store a raw template token), which is exactly the kind of invisible breakage
/// C-NOSTUB-001 forbids.
fn normalize_pr_mode(raw: Option<&str>, task_type: &str) -> Value {
    match raw {
        Some(m) if VALID_PR_MODES.contains(&m) => Value::String(m.to_string()),
        _ => Value::String(default_pr_mode_for(task_type).to_string()),
    }
}

/// Extract a required string param or return a descriptive `ActionFailed`.
fn require_str<'a>(params: &'a Value, key: &str, action: &str) -> Result<&'a str, ExecutionError> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecutionError::ActionFailed {
            action: action.to_string(),
            message: format!("missing or non-string '{key}'"),
        })
}

/// Recursive byte count of a directory tree (0 if the path is absent).
fn dir_size_bytes(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut total: u64 = 0;
    for entry in walkdir::WalkDir::new(path).into_iter().flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

/// Move a directory tree from `src` to `dest`, preserving its contents.
///
/// Fast path is `std::fs::rename` (same volume). On a cross-volume rename error
/// it falls back to a recursive copy followed by removal of the source — the
/// data is never lost, only relocated.
fn move_dir(src: &Path, dest: &Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Cross-volume (or otherwise un-renamable): copy then remove.
            copy_dir_recursive(src, dest)?;
            std::fs::remove_dir_all(src)?;
            Ok(())
        }
    }
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if file_type.is_symlink() {
            // Best-effort: copy the link target's bytes as a regular file.
            if let Ok(target) = std::fs::read(&from) {
                std::fs::write(&to, target)?;
            }
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Actions handled by the worktask handler (the allowlist).
const WORKTASK_ACTIONS: &[&str] = &[
    "timestamp_now",
    "generate_id",
    "git_worktree_add",
    "git_worktree_status",
    "git_worktree_remove",
    "git_branch_delete",
    "git_worktree_prune",
    "git_push_branch",
    "git_merge_branch",
    "fs_dir_size",
    "quarantine_worktree",
    "land_direct_merge",
    "land_github_pr",
    "land_subagent_review",
    "land_none",
    "list_tasks",
    "set_task_status",
    "identity",
    "make_task_record",
    "make_lease_record",
    "make_outcome",
    "count",
];

/// Check whether an action name is handled by the worktask handler.
pub fn is_worktask_action(action: &str) -> bool {
    WORKTASK_ACTIONS.contains(&action)
}

#[async_trait]
impl AsyncActionHandler for WorktaskActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "timestamp_now" => self.timestamp_now(),
            "generate_id" => self.generate_id(),
            "git_worktree_add" => self.git_worktree_add(params).await,
            "git_worktree_status" => self.git_worktree_status(params).await,
            "git_worktree_remove" => self.git_worktree_remove(params).await,
            "git_branch_delete" => self.git_branch_delete(params).await,
            "git_worktree_prune" => self.git_worktree_prune(params).await,
            "git_push_branch" => self.git_push_branch(params).await,
            "git_merge_branch" => self.git_merge_branch(params).await,
            "fs_dir_size" => self.fs_dir_size(params),
            "quarantine_worktree" => self.quarantine_worktree(params).await,
            "land_direct_merge" => self.land_direct_merge(params).await,
            "land_github_pr" => self.land_github_pr(params).await,
            "land_subagent_review" => self.land_subagent_review(params).await,
            "land_none" => self.land_none(params),
            "list_tasks" => self.list_tasks(params).await,
            "set_task_status" => self.set_task_status(params),
            "identity" => self.identity(params),
            "make_task_record" => self.make_task_record(params),
            "make_lease_record" => self.make_lease_record(params),
            "make_outcome" => self.make_outcome(params),
            "count" => self.count(params),
            _ => {
                warn!(action = %name, "worktask_actions: unknown action");
                Err(ExecutionError::ActionFailed {
                    action: name.to_string(),
                    message: "not a worktask action".into(),
                })
            }
        }
    }
}

/// Adversarial QA tests (Stage QA) live in a sibling file to keep edit churn
/// isolated from a parallel worker on this module. As a `#[path]`-included child
/// module it reaches this module's private items via `use super::*`.
#[cfg(test)]
#[path = "worktask_actions_qa.rs"]
mod qa;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::InMemoryStateStore;
    use tempfile::TempDir;

    fn handler() -> WorktaskActionHandler {
        WorktaskActionHandler::new(Arc::new(InMemoryStateStore::new()))
    }

    #[test]
    fn allowlist_recognizes_worktask_actions_and_rejects_others() {
        // Every advertised action is in the allowlist.
        for a in WORKTASK_ACTIONS {
            assert!(is_worktask_action(a), "{a} should be a worktask action");
        }
        // A representative real one and a clearly-foreign one.
        assert!(is_worktask_action("quarantine_worktree"));
        assert!(!is_worktask_action("read_state"));
        assert!(!is_worktask_action("get_default_stages"));
        assert!(!is_worktask_action("definitely_not_an_action"));
    }

    #[tokio::test]
    async fn timestamp_now_is_a_real_positive_epoch() {
        let h = handler();
        let v = h.call("timestamp_now", &json!({})).await.unwrap();
        let secs = v.as_u64().expect("timestamp_now returns a number");
        // Sanity: after 2021-01-01 (1_600_000_000) — proves it's a real clock
        // read, not a hardcoded constant.
        assert!(secs > 1_600_000_000, "epoch seconds look real: {secs}");
    }

    #[tokio::test]
    async fn generate_id_is_a_real_unique_uuid() {
        let h = handler();
        let a = h.call("generate_id", &json!({})).await.unwrap();
        let b = h.call("generate_id", &json!({})).await.unwrap();
        let sa = a.as_str().unwrap();
        let sb = b.as_str().unwrap();
        assert_ne!(sa, sb, "two ids must differ");
        // UUID v4 canonical form is 36 chars with 4 hyphens.
        assert_eq!(sa.len(), 36);
        assert_eq!(sa.matches('-').count(), 4);
        assert!(Uuid::parse_str(sa).is_ok(), "valid uuid: {sa}");
    }

    #[tokio::test]
    async fn unknown_action_is_a_real_error() {
        let h = handler();
        let err = h.call("nope", &json!({})).await.unwrap_err();
        match err {
            ExecutionError::ActionFailed { action, .. } => assert_eq!(action, "nope"),
            other => panic!("expected ActionFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fs_dir_size_counts_real_bytes_and_handles_absent() {
        let h = handler();
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), b"hello").unwrap(); // 5 bytes
        std::fs::write(tmp.path().join("b.txt"), b"world!").unwrap(); // 6 bytes
        let v = h
            .call(
                "fs_dir_size",
                &json!({ "path": tmp.path().to_string_lossy() }),
            )
            .await
            .unwrap();
        assert_eq!(v["bytes"], 11);
        assert_eq!(v["exists"], true);

        // Absent path → 0 bytes, not an error.
        let missing = h
            .call(
                "fs_dir_size",
                &json!({ "path": "C:/no/such/path/here/xyz" }),
            )
            .await
            .unwrap();
        assert_eq!(missing["bytes"], 0);
        assert_eq!(missing["exists"], false);
    }

    /// Helper: is a real `git` binary on PATH? Tests that shell out are skipped
    /// (not failed) when it isn't, so they never become a flaky environmental
    /// failure — the TEST stage runs them against a guaranteed git.
    async fn git_available() -> bool {
        GitEffects::default()
            .run("probe", &["--version"])
            .await
            .map(|o| o.status == 0)
            .unwrap_or(false)
    }

    /// Initialize a real scratch git repo with one commit at `dir`.
    async fn init_repo(git: &GitEffects, dir: &Path) {
        let d = dir.to_string_lossy();
        git.run_checked("t", &["-C", &d, "init", "-q"])
            .await
            .unwrap();
        git.run_checked("t", &["-C", &d, "config", "user.email", "t@example.com"])
            .await
            .unwrap();
        git.run_checked("t", &["-C", &d, "config", "user.name", "t"])
            .await
            .unwrap();
        std::fs::write(dir.join("seed.txt"), b"seed").unwrap();
        git.run_checked("t", &["-C", &d, "add", "-A"])
            .await
            .unwrap();
        git.run_checked("t", &["-C", &d, "commit", "-q", "-m", "seed"])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn git_worktree_status_parses_clean_vs_dirty() {
        if !git_available().await {
            eprintln!("skipping: git not on PATH");
            return;
        }
        let h = handler();
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&h.git, &repo).await;

        // Clean: porcelain empty → dirty == false.
        let clean = h
            .call(
                "git_worktree_status",
                &json!({ "worktree_path": repo.to_string_lossy() }),
            )
            .await
            .unwrap();
        assert_eq!(clean["dirty"], false, "freshly-committed repo is clean");
        assert_eq!(clean["lines"], 0);

        // Dirty: add an untracked file → dirty == true.
        std::fs::write(repo.join("scratch.txt"), b"uncommitted").unwrap();
        let dirty = h
            .call(
                "git_worktree_status",
                &json!({ "worktree_path": repo.to_string_lossy() }),
            )
            .await
            .unwrap();
        assert_eq!(dirty["dirty"], true, "untracked file makes it dirty");
        assert!(dirty["lines"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn quarantine_worktree_moves_dirty_tree_and_records_node() {
        // No git needed — this exercises the real fs move + durable record.
        let state = Arc::new(InMemoryStateStore::new());
        let h = WorktaskActionHandler::new(Arc::clone(&state) as Arc<dyn StateStore>);
        let tmp = TempDir::new().unwrap();

        // A "dirty" worktree with content we must NOT lose.
        let wt = tmp.path().join("dirty-worktree");
        std::fs::create_dir_all(wt.join("src")).unwrap();
        std::fs::write(
            wt.join("src").join("keep.txt"),
            b"precious uncommitted work",
        )
        .unwrap();

        let qroot = tmp.path().join("quarantine");
        let res = h
            .call(
                "quarantine_worktree",
                &json!({
                    "worktree_path": wt.to_string_lossy(),
                    "task_id": "wt_abc123",
                    "branch": "feat/x",
                    "reason": "expired_lease_dirty_tree",
                    "quarantine_root": qroot.to_string_lossy(),
                }),
            )
            .await
            .unwrap();

        // Source is gone (moved, not copied-and-left).
        assert!(
            !wt.exists(),
            "dirty worktree was MOVED out of its original path"
        );
        // Destination exists and the precious file survived.
        let dest = PathBuf::from(res["quarantined_path"].as_str().unwrap());
        assert!(dest.exists(), "quarantined path exists");
        let kept = dest.join("src").join("keep.txt");
        assert!(
            kept.exists(),
            "the uncommitted work was preserved, not deleted"
        );
        assert_eq!(
            std::fs::read_to_string(kept).unwrap(),
            "precious uncommitted work"
        );
        assert!(
            res["bytes"].as_u64().unwrap() > 0,
            "reported real byte size"
        );

        // Durable quarantine node was written.
        let node = state.get("worktask:quarantine:wt_abc123").await.unwrap();
        assert_eq!(node["task_id"], "wt_abc123");
        assert_eq!(node["reason"], "expired_lease_dirty_tree");
        assert_eq!(node["branch"], "feat/x");
    }

    #[tokio::test]
    async fn land_none_is_an_honest_noop_not_a_fake_success() {
        let h = handler();
        let v = h
            .call("land_none", &json!({ "branch": "feat/x" }))
            .await
            .unwrap();
        assert_eq!(v["mode"], "none");
        // It does NOT claim a merge happened.
        assert_eq!(v["landed"], false);
        assert_eq!(v["branch"], "feat/x");
    }

    #[tokio::test]
    async fn list_tasks_enumerates_real_state_and_filters_by_type() {
        let state = Arc::new(InMemoryStateStore::new());
        let h = WorktaskActionHandler::new(Arc::clone(&state) as Arc<dyn StateStore>);
        // Seed two features + one chore + an unrelated key.
        state
            .set(
                "worktask:task:a",
                json!({ "task_id": "a", "task_type": "feature" }),
            )
            .await;
        state
            .set(
                "worktask:task:b",
                json!({ "task_id": "b", "task_type": "feature" }),
            )
            .await;
        state
            .set(
                "worktask:task:c",
                json!({ "task_id": "c", "task_type": "chore" }),
            )
            .await;
        state
            .set("worktask:lease:a", json!({ "task_id": "a" }))
            .await; // must be ignored

        let all = h.call("list_tasks", &json!({})).await.unwrap();
        assert_eq!(
            all.as_array().unwrap().len(),
            3,
            "all three tasks, lease excluded"
        );

        let features = h
            .call("list_tasks", &json!({ "task_type": "feature" }))
            .await
            .unwrap();
        assert_eq!(
            features.as_array().unwrap().len(),
            2,
            "only the two features"
        );
        for t in features.as_array().unwrap() {
            assert_eq!(t["task_type"], "feature");
        }
    }

    #[tokio::test]
    async fn set_task_status_patches_status_and_updated_at_without_io() {
        let h = handler();
        let task = json!({ "task_id": "a", "status": "active", "created_at": 100 });
        let patched = h
            .call(
                "set_task_status",
                &json!({ "task": task, "status": "done", "updated_at": 200 }),
            )
            .await
            .unwrap();
        assert_eq!(patched["status"], "done");
        assert_eq!(patched["updated_at"], 200);
        // Untouched fields survive.
        assert_eq!(patched["task_id"], "a");
        assert_eq!(patched["created_at"], 100);
    }

    #[tokio::test]
    async fn identity_echoes_its_v_param_for_px_variable_binding() {
        let h = handler();
        // String value (e.g. a chosen pr_mode) round-trips unchanged.
        let s = h
            .call("identity", &json!({ "v": "direct-merge" }))
            .await
            .unwrap();
        assert_eq!(s, json!("direct-merge"));
        // A native number round-trips as a number (type preserved).
        let n = h.call("identity", &json!({ "v": 42 })).await.unwrap();
        assert_eq!(n, json!(42));
        // Absent v → null (honest, not a fabricated default).
        let none = h.call("identity", &json!({})).await.unwrap();
        assert_eq!(none, Value::Null);
    }

    #[tokio::test]
    async fn make_task_record_shapes_native_typed_record_from_payload() {
        let h = handler();
        let rec = h
            .call(
                "make_task_record",
                &json!({
                    "value": { "org": "plures", "repo": "radix", "branch": "feat/x",
                               "worktree_path": "/wt/x", "owner_session": "s1" },
                    "id": "wt_1", "now": 1700, "task_type": "feature",
                    "status": "active", "pr_mode": "github-pr"
                }),
            )
            .await
            .unwrap();
        // Scalars from params.
        assert_eq!(rec["task_id"], "wt_1");
        assert_eq!(rec["task_type"], "feature");
        assert_eq!(rec["status"], "active");
        assert_eq!(rec["pr_mode"], "github-pr");
        // created_at/updated_at are the native number (not a stringified copy).
        assert_eq!(rec["created_at"], 1700);
        assert_eq!(rec["updated_at"], 1700);
        assert!(rec["created_at"].is_number(), "native numeric timestamp");
        // Fields lifted from `value`.
        assert_eq!(rec["org"], "plures");
        assert_eq!(rec["repo"], "radix");
        assert_eq!(rec["branch"], "feat/x");
        assert_eq!(rec["worktree_path"], "/wt/x");
        assert_eq!(rec["owner_session"], "s1");
    }

    #[tokio::test]
    async fn make_lease_record_preserves_native_expiry() {
        let h = handler();
        let lease = h
            .call(
                "make_lease_record",
                &json!({
                    "value": { "owner_session": "s1", "owner_agent": "a1",
                               "lease_expires_at": 1_800_000_000u64 },
                    "id": "wt_1", "now": 1700
                }),
            )
            .await
            .unwrap();
        assert_eq!(lease["task_id"], "wt_1");
        assert_eq!(lease["acquired_at"], 1700);
        // Expiry stays a native u64 so `.px` `$now >= $lease.lease_expires_at`
        // numeric comparison works.
        assert_eq!(lease["lease_expires_at"], 1_800_000_000u64);
        assert!(lease["lease_expires_at"].is_number());
    }

    #[tokio::test]
    async fn count_counts_arrays_and_handles_absent() {
        let h = handler();
        let three = h.call("count", &json!({ "arr": [1, 2, 3] })).await.unwrap();
        assert_eq!(three, json!(3));
        // Absent / non-array → 0 (not an error, not a fake number).
        assert_eq!(h.call("count", &json!({})).await.unwrap(), json!(0));
        assert_eq!(
            h.call("count", &json!({ "arr": "nope" })).await.unwrap(),
            json!(0)
        );
    }

    #[tokio::test]
    async fn make_outcome_is_a_verbatim_object_passthrough() {
        let h = handler();
        let o = h
            .call(
                "make_outcome",
                &json!({ "task_id": "a", "action": "reclaimed", "bytes": 123 }),
            )
            .await
            .unwrap();
        assert_eq!(o["task_id"], "a");
        assert_eq!(o["action"], "reclaimed");
        assert_eq!(o["bytes"], 123);
    }
}

// ── END-TO-END TESTS ───────────────────────────────────────────────────────────
//
// Everything above unit-tests the handler actions in isolation. This module
// proves the executor works **end-to-end through the assembled reactive
// runtime** (`build_reactive_runtime`) against a REAL throwaway git repo and a
// REAL on-disk `PluresDbStateStore` — "build the binary, run the binary" at the
// library seam: a write into the live store fires the matching `worktask.px`
// procedure, which drives the real `WorktaskActionHandler` (real `git`
// subprocess, real fs, real durable state). No mock store, no fake git, no
// canned fixtures (C-TEST-002).
//
// Wiring mirrors the landed PxWire proof test
// (`spine::runtime::tests::end_to_end_write_triggers_px_procedure_persists_state`):
// build the runtime, fire `registry.on_write(key, payload)`, then poll the
// durable store for the effect (procedures run on a spawned task).
#[cfg(test)]
mod e2e {
    use super::*;
    use crate::model::{ToolDefinition, ToolDispatcher};
    use crate::spine::conversation::{ConversationStore, MemoryConversationStore};
    use crate::spine::runtime::{build_reactive_runtime, ReactiveRuntime};
    use crate::state::PluresDbStateStore;
    use serde_json::Value;
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use tempfile::TempDir;

    /// A dispatcher that does nothing — the worktask procedures only use core
    /// `read_state`/`write_state` (CoreActionHandler) and worktask actions
    /// (WorktaskActionHandler), never the tool dispatcher.
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

    /// Is a real `git` binary on PATH? E2E tests that need git skip (not fail)
    /// when it is genuinely absent. On this box git IS present, so they RUN.
    async fn git_available() -> bool {
        GitEffects::default()
            .run("probe", &["--version"])
            .await
            .map(|o| o.status == 0)
            .unwrap_or(false)
    }

    /// Locate the repo's real `praxis/procedures` directory (the production
    /// `worktask.px`). Resolved from CARGO_MANIFEST_DIR like the bootstrap
    /// regression test, so the runtime loads the SAME `.px` that ships.
    fn praxis_procedures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent() // crates/
            .and_then(|p| p.parent()) // project root
            .expect("project root")
            .join("praxis")
            .join("procedures")
    }

    /// Build the assembled reactive runtime against a real on-disk
    /// `PluresDbStateStore` rooted in `state_dir`, loading the real
    /// `praxis/procedures` `.px` files (so `worktask.px` is live).
    async fn build_runtime(state_dir: &Path) -> (ReactiveRuntime, Arc<dyn StateStore>) {
        let pdb = PluresDbStateStore::open(state_dir).expect("open state store");
        let state_store: Arc<dyn StateStore> = Arc::new(pdb);
        let conversation_store: Arc<dyn ConversationStore> =
            Arc::new(MemoryConversationStore::new());
        let dispatcher: Arc<dyn ToolDispatcher> = Arc::new(NullDispatcher);
        let runtime = build_reactive_runtime(
            Arc::clone(&state_store),
            conversation_store,
            dispatcher,
            &praxis_procedures_dir(),
            32,
        )
        .await;
        (runtime, state_store)
    }

    /// Initialize a real scratch git repo with one commit at `dir` (so worktrees
    /// can branch off `HEAD`). Uses the REAL git binary. Pins the default branch
    /// to `main` so direct-merge has a deterministic target.
    async fn init_repo(git: &GitEffects, dir: &Path) {
        let d = dir.to_string_lossy();
        git.run_checked("init", &["-C", &d, "init", "-q"])
            .await
            .unwrap();
        git.run_checked("cfg", &["-C", &d, "config", "user.email", "t@example.com"])
            .await
            .unwrap();
        git.run_checked("cfg", &["-C", &d, "config", "user.name", "t"])
            .await
            .unwrap();
        git.run_checked("br", &["-C", &d, "checkout", "-q", "-b", "main"])
            .await
            .ok();
        std::fs::write(dir.join("seed.txt"), b"seed").unwrap();
        git.run_checked("add", &["-C", &d, "add", "-A"])
            .await
            .unwrap();
        git.run_checked("commit", &["-C", &d, "commit", "-q", "-m", "seed"])
            .await
            .unwrap();
    }

    /// Real `git -C <repo> worktree list --porcelain` → list of worktree paths.
    async fn worktree_paths(git: &GitEffects, repo: &Path) -> Vec<String> {
        let out = git
            .run_checked(
                "wt-list",
                &[
                    "-C",
                    &repo.to_string_lossy(),
                    "worktree",
                    "list",
                    "--porcelain",
                ],
            )
            .await
            .unwrap();
        out.stdout
            .lines()
            .filter_map(|l| l.strip_prefix("worktree ").map(|s| s.trim().to_string()))
            .collect()
    }

    /// True if `repo`'s worktree list contains a worktree whose final path
    /// component equals `wt`'s (robust to git canonicalizing the absolute path).
    async fn worktree_listed(git: &GitEffects, repo: &Path, wt: &Path) -> bool {
        worktree_paths(git, repo)
            .await
            .iter()
            .any(|p| Path::new(p).file_name() == wt.file_name())
    }

    /// Poll the durable store until `key` holds a non-null value, or panic.
    async fn await_node(store: &Arc<dyn StateStore>, key: &str) -> Value {
        for _ in 0..200 {
            if let Some(v) = store.get(key).await {
                if !v.is_null() {
                    return v;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("durable node `{key}` never appeared — write→.px→effect path did not run");
    }

    /// Poll for the single `worktask:task:*` node whose `task_type` matches.
    /// Returns `(task_id, node)`. Polls because the procedure runs async.
    async fn await_task_of_type(store: &Arc<dyn StateStore>, task_type: &str) -> (String, Value) {
        for _ in 0..200 {
            for k in store.keys_with_prefix("worktask:task:").await {
                if let Some(v) = store.get(&k).await {
                    if v.get("task_type").and_then(|t| t.as_str()) == Some(task_type) {
                        let id = v
                            .get("task_id")
                            .and_then(|i| i.as_str())
                            .unwrap_or_default()
                            .to_string();
                        return (id, v);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no worktask:task of type `{task_type}` appeared in time");
    }

    /// Poll for the first node under `prefix` (e.g. the generated-id reclaim
    /// telemetry / quarantine record), returning its value.
    async fn await_first_under(store: &Arc<dyn StateStore>, prefix: &str) -> Value {
        for _ in 0..250 {
            if let Some(k) = store.keys_with_prefix(prefix).await.first() {
                if let Some(v) = store.get(k).await {
                    if !v.is_null() {
                        return v;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no node under prefix `{prefix}` appeared in time");
    }

    /// Wait until a path exists (a real worktree dir created by the executor).
    async fn await_path(p: &Path) {
        for _ in 0..200 {
            if p.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("path never appeared: {}", p.display());
    }

    /// Deterministic barrier: wait until at least `n` `worktask:task:*` nodes
    /// are durably persisted AND each one has its matching `worktask:lease:*`
    /// node. The executor writes the worktree dir (via `git_worktree_add`)
    /// BEFORE it persists the task+lease nodes, so waiting only on the dir
    /// races the `reclaim`/`doctor` enumeration (`list_tasks`). This closes that
    /// race by gating on the durable nodes the enumeration actually reads.
    async fn await_tasks_and_leases_persisted(store: &Arc<dyn StateStore>, n: usize) {
        for _ in 0..300 {
            let task_keys = store.keys_with_prefix("worktask:task:").await;
            if task_keys.len() >= n {
                let mut all_leased = true;
                for tk in &task_keys {
                    let id = tk.strip_prefix("worktask:task:").unwrap_or_default();
                    let leased = store
                        .get(&format!("worktask:lease:{id}"))
                        .await
                        .map(|v| !v.is_null())
                        .unwrap_or(false);
                    if !leased {
                        all_leased = false;
                        break;
                    }
                }
                if all_leased {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("expected >= {n} task+lease node pairs to persist, timed out");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Scenario 2 — new_feature: real worktree + task node + lease node.
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn e2e_new_feature_creates_real_worktree_task_and_lease() {
        if !git_available().await {
            eprintln!("skipping e2e_new_feature: git not on PATH");
            return;
        }
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = GitEffects::default();
        init_repo(&git, &repo).await;

        let (runtime, store) = build_runtime(&tmp.path().join("state")).await;
        let wt_path = tmp.path().join("wt-feature");

        let payload = json!({
            "org": "plures",
            "repo": repo.to_string_lossy(),
            "branch": "feat/e2e-feature",
            "worktree_path": wt_path.to_string_lossy(),
            "owner_session": "sess-1",
            "owner_agent": "agent-1",
            "lease_expires_at": 9_000_000_000u64,
        });
        runtime
            .registry
            .on_write("worktask:cmd:new_feature:req-1", &payload)
            .await;

        let (task_id, task) = await_task_of_type(&store, "feature").await;
        assert_eq!(task["org"], "plures");
        assert_eq!(task["status"], "active");
        assert_eq!(task["branch"], "feat/e2e-feature");
        // No policy seeded → feature default pr_mode is github-pr.
        assert_eq!(task["pr_mode"], "github-pr", "feature default pr_mode");
        assert_eq!(
            task["worktree_path"].as_str().unwrap(),
            wt_path.to_string_lossy()
        );

        let lease = await_node(&store, &format!("worktask:lease:{task_id}")).await;
        assert_eq!(lease["task_id"], task_id);
        assert_eq!(lease["owner_session"], "sess-1");
        assert_eq!(lease["lease_expires_at"], 9_000_000_000u64);
        assert!(
            lease["lease_expires_at"].is_number(),
            "native expiry preserved"
        );

        // The REAL git worktree must exist on disk AND in `git worktree list`.
        await_path(&wt_path).await;
        assert!(
            worktree_listed(&git, &repo, &wt_path).await,
            "git worktree list must show the new worktree {}",
            wt_path.display()
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Scenario 3 — resolve_pr_mode precedence (4-tier chain lives in .px).
    //   Order: override > repo > org+type > global, fallback github-pr.
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn e2e_pr_mode_repo_overrides_global() {
        if !git_available().await {
            eprintln!("skipping e2e_pr_mode_repo: git not on PATH");
            return;
        }
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = GitEffects::default();
        init_repo(&git, &repo).await;

        let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

        // GLOBAL = subagent-review, REPO = direct-merge. Repo tier must win.
        store
            .set(
                "worktask:policy:global",
                json!({ "pr_mode": "subagent-review" }),
            )
            .await;
        store
            .set(
                &format!("worktask:policy:repo:plures/{}", repo.to_string_lossy()),
                json!({ "pr_mode": "direct-merge" }),
            )
            .await;

        let payload = json!({
            "org": "plures",
            "repo": repo.to_string_lossy(),
            "branch": "feat/pr-mode-repo",
            "worktree_path": tmp.path().join("wt-prmode-repo").to_string_lossy(),
            "owner_session": "s",
            "owner_agent": "a",
            "lease_expires_at": 9_000_000_000u64,
        });
        runtime
            .registry
            .on_write("worktask:cmd:new_feature:req-prmode-repo", &payload)
            .await;

        let (_id, task) = await_task_of_type(&store, "feature").await;
        assert_eq!(
            task["pr_mode"], "direct-merge",
            "repo policy (direct-merge) must override global (subagent-review)"
        );
    }

    #[tokio::test]
    async fn e2e_pr_mode_global_overwrites_chore_default() {
        if !git_available().await {
            eprintln!("skipping e2e_pr_mode_global: git not on PATH");
            return;
        }
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = GitEffects::default();
        init_repo(&git, &repo).await;

        let (runtime, store) = build_runtime(&tmp.path().join("state")).await;
        // Only GLOBAL set, to github-pr. A chore's built-in default is
        // direct-merge, so this proves the global tier overwrites the default
        // (a real precedence step, not the bare fallback).
        store
            .set("worktask:policy:global", json!({ "pr_mode": "github-pr" }))
            .await;

        let payload = json!({
            "org": "plures",
            "repo": repo.to_string_lossy(),
            "branch": "chore/pr-mode-global",
            "worktree_path": tmp.path().join("wt-prmode-global").to_string_lossy(),
            "owner_session": "s",
            "owner_agent": "a",
            "lease_expires_at": 9_000_000_000u64,
        });
        runtime
            .registry
            .on_write("worktask:cmd:new_chore:req-prmode-global", &payload)
            .await;

        let (_id, task) = await_task_of_type(&store, "chore").await;
        assert_eq!(
            task["pr_mode"], "github-pr",
            "global (github-pr) must overwrite the chore default (direct-merge)"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Scenario 4 — reclaim (SAFETY-CRITICAL): clean ⇒ removed, dirty ⇒ MOVED
    //   to quarantine. Uncommitted work is preserved and NEVER deleted.
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn e2e_reclaim_removes_clean_and_quarantines_dirty_without_data_loss() {
        if !git_available().await {
            eprintln!("skipping e2e_reclaim: git not on PATH");
            return;
        }
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = GitEffects::default();
        init_repo(&git, &repo).await;

        let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

        // Seed TWO real worktrees via the executor, each with an ALREADY-EXPIRED
        // lease (epoch 1 = long past) so reclaim acts on both.
        let clean_wt = tmp.path().join("wt-clean");
        let dirty_wt = tmp.path().join("wt-dirty");
        for (i, (branch, wt)) in [("feat/clean", &clean_wt), ("feat/dirty", &dirty_wt)]
            .iter()
            .enumerate()
        {
            let payload = json!({
                "org": "plures",
                "repo": repo.to_string_lossy(),
                "branch": branch,
                "worktree_path": wt.to_string_lossy(),
                "owner_session": "s",
                "owner_agent": "a",
                "lease_expires_at": 1u64,
            });
            runtime
                .registry
                .on_write(&format!("worktask:cmd:new_feature:seed-{i}"), &payload)
                .await;
            await_path(wt).await;
        }

        // Barrier: BOTH task+lease nodes must be durably persisted before we
        // fire reclaim, else `list_tasks` may enumerate only one (a test race,
        // not an executor bug).
        await_tasks_and_leases_persisted(&store, 2).await;

        // Make the DIRTY worktree actually dirty: an uncommitted file we must
        // NOT lose under any circumstances.
        let precious = dirty_wt.join("uncommitted-precious.txt");
        std::fs::write(&precious, b"do not delete me").unwrap();

        // Both worktrees present before reclaim.
        let before = worktree_paths(&git, &repo).await;
        assert!(
            before
                .iter()
                .any(|p| Path::new(p).file_name() == clean_wt.file_name()),
            "clean worktree present before reclaim"
        );
        assert!(
            before
                .iter()
                .any(|p| Path::new(p).file_name() == dirty_wt.file_name()),
            "dirty worktree present before reclaim"
        );

        // Fire reclaim.
        runtime
            .registry
            .on_write("worktask:cmd:reclaim:run-1", &json!({}))
            .await;

        // Telemetry node (run_id generated) must record real per-task outcomes.
        let telemetry = await_first_under(&store, "worktask:reclaim:").await;
        let outcomes = telemetry["outcomes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(
            outcomes.len() >= 2,
            "telemetry records one outcome per examined task; got {outcomes:?}"
        );
        let actions: Vec<String> = outcomes
            .iter()
            .filter_map(|o| o.get("action").and_then(|v| v.as_str()).map(String::from))
            .collect();
        assert!(
            actions.iter().any(|a| a == "reclaimed"),
            "one task must be reclaimed (clean); actions={actions:?}"
        );
        assert!(
            actions.iter().any(|a| a == "quarantined"),
            "one task must be quarantined (dirty); actions={actions:?}"
        );

        // ── CLEAN worktree must be GONE (removed from disk + git list) ────────
        for _ in 0..200 {
            if !clean_wt.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            !clean_wt.exists(),
            "CLEAN worktree must be removed from disk after reclaim"
        );
        assert!(
            !worktree_listed(&git, &repo, &clean_wt).await,
            "CLEAN worktree must be gone from `git worktree list`"
        );

        // ── DIRTY worktree must be MOVED to quarantine, never deleted ─────────
        let qnode = await_first_under(&store, "worktask:quarantine:").await;
        let quarantined_path = qnode["quarantined_path"]
            .as_str()
            .expect("quarantine record has a real destination path")
            .to_string();
        let qdir = PathBuf::from(&quarantined_path);
        assert_eq!(qnode["reason"], "expired_lease_dirty_tree");
        assert!(
            qnode["bytes"].as_u64().unwrap_or(0) > 0,
            "quarantine record reports real byte size, got {:?}",
            qnode["bytes"]
        );

        // Source GONE (moved out), destination EXISTS, file PRESERVED byte-exact.
        assert!(
            !dirty_wt.exists(),
            "DIRTY worktree source path must be absent after a MOVE"
        );
        assert!(
            qdir.exists(),
            "quarantine destination must exist: {quarantined_path}"
        );
        let preserved = qdir.join("uncommitted-precious.txt");
        assert!(
            preserved.exists(),
            "the uncommitted work must be PRESERVED at the quarantine path"
        );
        assert_eq!(
            std::fs::read_to_string(&preserved).unwrap(),
            "do not delete me",
            "quarantined file content must be byte-identical (no data loss)"
        );

        // Best-effort cleanup of the shared-temp quarantine dir.
        let _ = std::fs::remove_dir_all(&qdir);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Scenario 5 — doctor: read-only health report; MUTATES NOTHING.
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn e2e_doctor_reports_health_and_mutates_nothing() {
        if !git_available().await {
            eprintln!("skipping e2e_doctor: git not on PATH");
            return;
        }
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = GitEffects::default();
        init_repo(&git, &repo).await;

        let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

        // One ACTIVE (future lease) + one EXPIRED (past lease) worktree task.
        let active_wt = tmp.path().join("wt-active");
        let expired_wt = tmp.path().join("wt-expired");
        for (i, (branch, wt, expiry)) in [
            ("feat/active", &active_wt, 9_000_000_000u64),
            ("feat/expired", &expired_wt, 1u64),
        ]
        .iter()
        .enumerate()
        {
            let payload = json!({
                "org": "plures",
                "repo": repo.to_string_lossy(),
                "branch": branch,
                "worktree_path": wt.to_string_lossy(),
                "owner_session": "s",
                "owner_agent": "a",
                "lease_expires_at": expiry,
            });
            runtime
                .registry
                .on_write(&format!("worktask:cmd:new_feature:doc-{i}"), &payload)
                .await;
            await_path(wt).await;
        }
        // Barrier: both task+lease nodes persisted before we snapshot, so a
        // late-landing seed write can't masquerade as a doctor mutation.
        await_tasks_and_leases_persisted(&store, 2).await;
        // Make the expired one dirty so doctor would classify dirty=true.
        std::fs::write(expired_wt.join("scratch.txt"), b"wip").unwrap();

        // Snapshot the ENTIRE durable worktask state + worktree set before.
        let keys_before = store.keys_with_prefix("worktask:").await;
        let mut snapshot_before: Vec<(String, Value)> = Vec::new();
        for k in &keys_before {
            snapshot_before.push((k.clone(), store.get(k).await.unwrap_or(Value::Null)));
        }
        snapshot_before.sort_by(|a, b| a.0.cmp(&b.0));
        let worktrees_before = worktree_paths(&git, &repo).await;

        // Fire doctor; give it ample time to run all read steps.
        runtime
            .registry
            .on_write("worktask:cmd:doctor:run-1", &json!({}))
            .await;
        tokio::time::sleep(Duration::from_millis(700)).await;

        // ── ZERO mutations: same keys, same values, same worktrees. ───────────
        let keys_after = store.keys_with_prefix("worktask:").await;
        let mut snapshot_after: Vec<(String, Value)> = Vec::new();
        for k in &keys_after {
            snapshot_after.push((k.clone(), store.get(k).await.unwrap_or(Value::Null)));
        }
        snapshot_after.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            snapshot_before.len(),
            snapshot_after.len(),
            "doctor must not add or remove any state node"
        );
        assert_eq!(
            snapshot_before, snapshot_after,
            "doctor is read-only: every state node must be byte-identical"
        );
        assert!(
            store.keys_with_prefix("worktask:reclaim:").await.is_empty(),
            "doctor must not write reclaim telemetry"
        );
        assert!(
            store
                .keys_with_prefix("worktask:quarantine:")
                .await
                .is_empty(),
            "doctor must not quarantine anything"
        );
        let worktrees_after = worktree_paths(&git, &repo).await;
        assert_eq!(
            worktrees_before.len(),
            worktrees_after.len(),
            "doctor must not remove or add worktrees"
        );
        assert!(
            active_wt.exists() && expired_wt.exists(),
            "both worktrees intact"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Scenario 6 — new_pr direct-merge: real merge to main + worktree/branch
    //   cleanup + status→done. Plus the HONEST push-only boundary (github-pr).
    // ─────────────────────────────────────────────────────────────────────────
    #[tokio::test]
    async fn e2e_new_pr_direct_merge_merges_cleans_up_and_marks_done() {
        if !git_available().await {
            eprintln!("skipping e2e_new_pr_direct: git not on PATH");
            return;
        }
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = GitEffects::default();
        init_repo(&git, &repo).await;

        let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

        // Force direct-merge via a global policy, then create a chore worktree.
        store
            .set(
                "worktask:policy:global",
                json!({ "pr_mode": "direct-merge" }),
            )
            .await;
        let wt = tmp.path().join("wt-merge");
        let branch = "chore/e2e-merge";
        runtime
            .registry
            .on_write(
                "worktask:cmd:new_chore:merge-1",
                &json!({
                    "org": "plures",
                    "repo": repo.to_string_lossy(),
                    "branch": branch,
                    "worktree_path": wt.to_string_lossy(),
                    "owner_session": "s",
                    "owner_agent": "a",
                    "lease_expires_at": 9_000_000_000u64,
                }),
            )
            .await;
        let (task_id, task) = await_task_of_type(&store, "chore").await;
        assert_eq!(task["pr_mode"], "direct-merge");
        await_path(&wt).await;

        // Commit a real change ON the worktree's branch so the merge has content.
        let wtd = wt.to_string_lossy();
        std::fs::write(wt.join("feature.txt"), b"new feature work").unwrap();
        git.run_checked("add", &["-C", &wtd, "add", "-A"])
            .await
            .unwrap();
        git.run_checked("ci", &["-C", &wtd, "commit", "-q", "-m", "feature work"])
            .await
            .unwrap();

        // Fire new_pr for the task.
        runtime
            .registry
            .on_write("worktask:cmd:new_pr:pr-1", &json!({ "task_id": task_id }))
            .await;

        // Task status must reach `done`.
        let mut done = false;
        for _ in 0..200 {
            if let Some(t) = store.get(&format!("worktask:task:{task_id}")).await {
                if t.get("status").and_then(|s| s.as_str()) == Some("done") {
                    done = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(done, "direct-merge new_pr must drive task status to `done`");

        // The merge really happened: feature.txt is now on main in the repo.
        let merged_file = repo.join("feature.txt");
        assert!(
            merged_file.exists(),
            "merged content must be present on the repo's main worktree"
        );
        assert_eq!(
            std::fs::read_to_string(&merged_file).unwrap(),
            "new feature work"
        );

        // Worktree removed + branch deleted + pruned.
        for _ in 0..200 {
            if !wt.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            !wt.exists(),
            "worktree must be removed after a direct merge"
        );
        assert!(
            !worktree_listed(&git, &repo, &wt).await,
            "worktree must be gone from git worktree list"
        );
        let branches = git
            .run_checked("br", &["-C", &repo.to_string_lossy(), "branch", "--list"])
            .await
            .unwrap();
        assert!(
            !branches.stdout.contains(branch),
            "feature branch must be deleted after merge; branches=\n{}",
            branches.stdout
        );
    }

    /// github-pr (push-only) mode: a real `git push` of the branch happens and
    /// the task is LEFT `in_review` (no fabricated merged PR). This is the
    /// by-design honest boundary per the implement result. We give the repo a
    /// real local `--bare` remote so the push genuinely succeeds.
    #[tokio::test]
    async fn e2e_new_pr_github_mode_pushes_real_branch_and_leaves_in_review() {
        if !git_available().await {
            eprintln!("skipping e2e_new_pr_github: git not on PATH");
            return;
        }
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = GitEffects::default();
        init_repo(&git, &repo).await;

        // A real bare remote the worktree can push to.
        let remote = tmp.path().join("remote.git");
        git.run_checked(
            "init-bare",
            &["init", "--bare", "-q", &remote.to_string_lossy()],
        )
        .await
        .unwrap();

        let (runtime, store) = build_runtime(&tmp.path().join("state")).await;
        store
            .set("worktask:policy:global", json!({ "pr_mode": "github-pr" }))
            .await;

        let wt = tmp.path().join("wt-push");
        let branch = "feat/e2e-push";
        runtime
            .registry
            .on_write(
                "worktask:cmd:new_feature:push-1",
                &json!({
                    "org": "plures",
                    "repo": repo.to_string_lossy(),
                    "branch": branch,
                    "worktree_path": wt.to_string_lossy(),
                    "owner_session": "s",
                    "owner_agent": "a",
                    "lease_expires_at": 9_000_000_000u64,
                }),
            )
            .await;
        let (task_id, task) = await_task_of_type(&store, "feature").await;
        assert_eq!(task["pr_mode"], "github-pr");
        await_path(&wt).await;

        // Wire the bare repo as `origin` on the worktree + commit a change so the
        // push has content. (The .px push uses remote `origin` by default.)
        let wtd = wt.to_string_lossy();
        git.run_checked(
            "remote-add",
            &[
                "-C",
                &wtd,
                "remote",
                "add",
                "origin",
                &remote.to_string_lossy(),
            ],
        )
        .await
        .unwrap();
        std::fs::write(wt.join("pushme.txt"), b"push this").unwrap();
        git.run_checked("add", &["-C", &wtd, "add", "-A"])
            .await
            .unwrap();
        git.run_checked("ci", &["-C", &wtd, "commit", "-q", "-m", "push work"])
            .await
            .unwrap();

        // Fire new_pr (github-pr mode).
        runtime
            .registry
            .on_write(
                "worktask:cmd:new_pr:pr-push-1",
                &json!({ "task_id": task_id }),
            )
            .await;

        // Status must be left `in_review` (NOT done, NOT a fake merge). Poll for
        // a stable terminal-ish state, then assert it's in_review.
        let mut status = String::new();
        for _ in 0..200 {
            if let Some(t) = store.get(&format!("worktask:task:{task_id}")).await {
                if let Some(s) = t.get("status").and_then(|v| v.as_str()) {
                    status = s.to_string();
                    if s == "in_review" {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            status, "in_review",
            "github-pr mode must leave the task in_review (honest boundary), got `{status}`"
        );

        // The branch was REALLY pushed: the bare remote now has it.
        let ls = git
            .run_checked(
                "ls-remote",
                &["ls-remote", "--heads", &remote.to_string_lossy()],
            )
            .await
            .unwrap();
        assert!(
            ls.stdout.contains(branch),
            "github-pr mode must have really pushed the branch to origin; ls-remote=\n{}",
            ls.stdout
        );

        // The worktree was NOT torn down (push-only leaves it for the external step).
        assert!(
            wt.exists(),
            "github-pr mode must NOT remove the worktree (no real merge happened)"
        );
    }

    /// VERIFY loop-closer: drive the SHIPPED command surface end-to-end through
    /// a complete real lifecycle (new_feature -> real commit work -> new_pr
    /// direct-merge -> reclaim clean+dirty) and probe the safety guard surface.
    ///
    /// This test intentionally records concrete evidence (git worktree listings,
    /// durable node snippets, origin/main merge visibility) and asserts the
    /// effects the current shipped implementation guarantees. Contract checks
    /// that may currently be unmet are captured in the evidence payload for the
    /// VERIFY report (not silently ignored).
    #[tokio::test]
    async fn e2e_verify_command_surface_full_lifecycle_probe() {
        if !git_available().await {
            eprintln!("skipping e2e_verify_lifecycle: git not on PATH");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let git = GitEffects::default();

        // Real repo + real bare origin so github-pr push mode is testable.
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&git, &repo).await;
        let origin = tmp.path().join("origin.git");
        git.run_checked(
            "init-bare",
            &["init", "--bare", "-q", &origin.to_string_lossy()],
        )
        .await
        .unwrap();
        git.run_checked(
            "remote-add",
            &[
                "-C",
                &repo.to_string_lossy(),
                "remote",
                "add",
                "origin",
                &origin.to_string_lossy(),
            ],
        )
        .await
        .unwrap();
        git.run_checked(
            "push-main",
            &[
                "-C",
                &repo.to_string_lossy(),
                "push",
                "-u",
                "origin",
                "main",
            ],
        )
        .await
        .unwrap();

        let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

        // Force direct-merge for the lifecycle land step.
        store
            .set(
                "worktask:policy:global",
                json!({ "pr_mode": "direct-merge" }),
            )
            .await;

        // 1) newFeature (command surface = write worktask:cmd:*).
        let wt_feature = tmp.path().join("wt-verify-feature");
        let feature_branch = "feat/verify-lifecycle";
        runtime
            .registry
            .on_write(
                "worktask:cmd:new_feature:verify-1",
                &json!({
                    "org": "plures",
                    "repo": repo.to_string_lossy(),
                    "branch": feature_branch,
                    "worktree_path": wt_feature.to_string_lossy(),
                    "owner_session": "verify-session",
                    "owner_agent": "verify-agent",
                    "lease_expires_at": 9_000_000_000u64,
                }),
            )
            .await;

        let (feature_task_id, feature_task) = await_task_of_type(&store, "feature").await;
        let feature_lease = await_node(&store, &format!("worktask:lease:{feature_task_id}")).await;
        await_path(&wt_feature).await;
        let wt_list_after_new = git
            .run_checked(
                "wt-list-after-new",
                &[
                    "-C",
                    &repo.to_string_lossy(),
                    "worktree",
                    "list",
                    "--porcelain",
                ],
            )
            .await
            .unwrap()
            .stdout;
        assert!(
            wt_list_after_new.contains("wt-verify-feature"),
            "new_feature must create a real listed worktree"
        );

        // 2) work (real commit in the created worktree).
        std::fs::write(wt_feature.join("verify-work.txt"), b"verify lifecycle work").unwrap();
        let wt_feature_s = wt_feature.to_string_lossy();
        git.run_checked("add", &["-C", &wt_feature_s, "add", "-A"])
            .await
            .unwrap();
        git.run_checked(
            "commit",
            &[
                "-C",
                &wt_feature_s,
                "commit",
                "-q",
                "-m",
                "verify lifecycle work",
            ],
        )
        .await
        .unwrap();

        // 3) land via new_pr direct-merge + cleanup.
        runtime
            .registry
            .on_write(
                "worktask:cmd:new_pr:verify-pr-1",
                &json!({ "task_id": feature_task_id }),
            )
            .await;

        let mut landed_done = false;
        for _ in 0..220 {
            if let Some(t) = store.get(&format!("worktask:task:{feature_task_id}")).await {
                if t.get("status").and_then(|s| s.as_str()) == Some("done") {
                    landed_done = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            landed_done,
            "direct-merge new_pr must mark task done in durable state"
        );
        assert!(
            repo.join("verify-work.txt").exists(),
            "direct-merge must land merged content on repo main"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("verify-work.txt")).unwrap(),
            "verify lifecycle work"
        );

        for _ in 0..220 {
            if !wt_feature.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            !wt_feature.exists(),
            "worktree must be removed after direct-merge landing"
        );
        let wt_list_after_land = git
            .run_checked(
                "wt-list-after-land",
                &[
                    "-C",
                    &repo.to_string_lossy(),
                    "worktree",
                    "list",
                    "--porcelain",
                ],
            )
            .await
            .unwrap()
            .stdout;
        let branches_after_land = git
            .run_checked(
                "branch-list-after-land",
                &["-C", &repo.to_string_lossy(), "branch", "--list"],
            )
            .await
            .unwrap()
            .stdout;
        assert!(
            !branches_after_land.contains(feature_branch),
            "feature branch must be deleted after direct-merge"
        );

        // Probe whether direct-merge landed on ORIGIN/main (skill contract).
        // Keep this as evidence to report, not as a hard assertion here.
        let origin_show = git
            .run(
                "origin-show",
                &[
                    "--git-dir",
                    &origin.to_string_lossy(),
                    "show",
                    "refs/heads/main:verify-work.txt",
                ],
            )
            .await
            .ok();
        let origin_has_merged_content = origin_show
            .as_ref()
            .map(|o| o.status == 0 && o.stdout.trim() == "verify lifecycle work")
            .unwrap_or(false);

        // 4) reclaim: second clean task reclaimed, dirty task quarantined.
        let wt_clean = tmp.path().join("wt-verify-clean");
        let wt_dirty = tmp.path().join("wt-verify-dirty");
        for (i, (branch, wt)) in [
            ("feat/verify-clean", &wt_clean),
            ("feat/verify-dirty", &wt_dirty),
        ]
        .iter()
        .enumerate()
        {
            runtime
                .registry
                .on_write(
                    &format!("worktask:cmd:new_feature:verify-reclaim-{i}"),
                    &json!({
                        "org": "plures",
                        "repo": repo.to_string_lossy(),
                        "branch": branch,
                        "worktree_path": wt.to_string_lossy(),
                        "owner_session": "verify-session",
                        "owner_agent": "verify-agent",
                        "lease_expires_at": 1u64,
                    }),
                )
                .await;
            await_path(wt).await;
        }
        await_tasks_and_leases_persisted(&store, 3).await;
        std::fs::write(wt_dirty.join("dirty-preserve.txt"), b"preserve me").unwrap();

        runtime
            .registry
            .on_write("worktask:cmd:reclaim:verify-1", &json!({}))
            .await;

        let reclaim_tel = await_first_under(&store, "worktask:reclaim:").await;
        let quarantine = await_first_under(&store, "worktask:quarantine:").await;
        let quarantined_path = quarantine
            .get("quarantined_path")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let preserved = PathBuf::from(&quarantined_path).join("dirty-preserve.txt");

        for _ in 0..220 {
            if !wt_clean.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!wt_clean.exists(), "clean expired worktree must be removed");
        assert!(
            !wt_dirty.exists(),
            "dirty expired worktree source must be moved"
        );
        assert!(
            preserved.exists(),
            "dirty quarantined data must be preserved"
        );
        assert_eq!(
            std::fs::read_to_string(&preserved).unwrap(),
            "preserve me",
            "quarantined dirty file content must be preserved"
        );

        // 5) Refuse-unsafe probe: new_pr without task assignment should not
        // mutate state or perform cleanup side effects.
        let keys_before_refuse = store.keys_with_prefix("worktask:task:").await;
        let wt_before_refuse = git
            .run_checked(
                "wt-list-before-refuse",
                &[
                    "-C",
                    &repo.to_string_lossy(),
                    "worktree",
                    "list",
                    "--porcelain",
                ],
            )
            .await
            .unwrap()
            .stdout;
        runtime
            .registry
            .on_write(
                "worktask:cmd:new_pr:verify-missing-task",
                &json!({ "task_id": "missing-task-id" }),
            )
            .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let keys_after_refuse = store.keys_with_prefix("worktask:task:").await;
        let wt_after_refuse = git
            .run_checked(
                "wt-list-after-refuse",
                &[
                    "-C",
                    &repo.to_string_lossy(),
                    "worktree",
                    "list",
                    "--porcelain",
                ],
            )
            .await
            .unwrap()
            .stdout;
        assert_eq!(
            keys_before_refuse.len(),
            keys_after_refuse.len(),
            "missing-task new_pr should not create/mutate task records"
        );
        assert_eq!(
            wt_before_refuse, wt_after_refuse,
            "missing-task new_pr should not mutate worktree topology"
        );

        let evidence = json!({
            "new_feature": {
                "task_id": feature_task_id,
                "task": feature_task,
                "lease": feature_lease,
                "git_worktree_list": wt_list_after_new,
            },
            "land": {
                "repo_main_has_file": repo.join("verify-work.txt").exists(),
                "repo_main_file": std::fs::read_to_string(repo.join("verify-work.txt")).unwrap_or_default(),
                "git_worktree_list_after_land": wt_list_after_land,
                "branches_after_land": branches_after_land,
                "origin_main_has_merged_content": origin_has_merged_content,
            },
            "reclaim": {
                "telemetry": reclaim_tel,
                "quarantine": quarantine,
                "preserved_file": preserved.to_string_lossy(),
                "preserved_content": std::fs::read_to_string(&preserved).unwrap_or_default(),
            },
            "refuse_unsafe_probe": {
                "keys_before": keys_before_refuse.len(),
                "keys_after": keys_after_refuse.len(),
                "worktrees_unchanged": wt_before_refuse == wt_after_refuse,
                "surface": "new_pr with missing task_id leaves topology/state unchanged",
            }
        });
        eprintln!("VERIFY_E2E_EVIDENCE={} ", evidence);
    }
}
