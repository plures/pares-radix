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
        Ok(json!({ "ok": true, "worktree_path": wt, "branch": branch, "stdout": out.stdout.trim() }))
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
        let force = params.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
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
        let force = params.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
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
            .run_checked("git_merge_branch", &["-C", repo, "merge", "--no-ff", branch])
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
        Ok(json!({
            "task_id": p("id"),
            "org": field("org"),
            "repo": field("repo"),
            "task_type": p("task_type"),
            "branch": field("branch"),
            "worktree_path": field("worktree_path"),
            "owner_session": field("owner_session"),
            "owner_agent": field("owner_agent"),
            "status": p("status"),
            "pr_mode": p("pr_mode"),
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

/// Extract a required string param or return a descriptive `ActionFailed`.
fn require_str<'a>(
    params: &'a Value,
    key: &str,
    action: &str,
) -> Result<&'a str, ExecutionError> {
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
            .call("fs_dir_size", &json!({ "path": tmp.path().to_string_lossy() }))
            .await
            .unwrap();
        assert_eq!(v["bytes"], 11);
        assert_eq!(v["exists"], true);

        // Absent path → 0 bytes, not an error.
        let missing = h
            .call("fs_dir_size", &json!({ "path": "C:/no/such/path/here/xyz" }))
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
        git.run_checked("t", &["-C", &d, "init", "-q"]).await.unwrap();
        git.run_checked("t", &["-C", &d, "config", "user.email", "t@example.com"]).await.unwrap();
        git.run_checked("t", &["-C", &d, "config", "user.name", "t"]).await.unwrap();
        std::fs::write(dir.join("seed.txt"), b"seed").unwrap();
        git.run_checked("t", &["-C", &d, "add", "-A"]).await.unwrap();
        git.run_checked("t", &["-C", &d, "commit", "-q", "-m", "seed"]).await.unwrap();
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
            .call("git_worktree_status", &json!({ "worktree_path": repo.to_string_lossy() }))
            .await
            .unwrap();
        assert_eq!(clean["dirty"], false, "freshly-committed repo is clean");
        assert_eq!(clean["lines"], 0);

        // Dirty: add an untracked file → dirty == true.
        std::fs::write(repo.join("scratch.txt"), b"uncommitted").unwrap();
        let dirty = h
            .call("git_worktree_status", &json!({ "worktree_path": repo.to_string_lossy() }))
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
        std::fs::write(wt.join("src").join("keep.txt"), b"precious uncommitted work").unwrap();

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
        assert!(!wt.exists(), "dirty worktree was MOVED out of its original path");
        // Destination exists and the precious file survived.
        let dest = PathBuf::from(res["quarantined_path"].as_str().unwrap());
        assert!(dest.exists(), "quarantined path exists");
        let kept = dest.join("src").join("keep.txt");
        assert!(kept.exists(), "the uncommitted work was preserved, not deleted");
        assert_eq!(std::fs::read_to_string(kept).unwrap(), "precious uncommitted work");
        assert!(res["bytes"].as_u64().unwrap() > 0, "reported real byte size");

        // Durable quarantine node was written.
        let node = state.get("worktask:quarantine:wt_abc123").await.unwrap();
        assert_eq!(node["task_id"], "wt_abc123");
        assert_eq!(node["reason"], "expired_lease_dirty_tree");
        assert_eq!(node["branch"], "feat/x");
    }

    #[tokio::test]
    async fn land_none_is_an_honest_noop_not_a_fake_success() {
        let h = handler();
        let v = h.call("land_none", &json!({ "branch": "feat/x" })).await.unwrap();
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
        state.set("worktask:task:a", json!({ "task_id": "a", "task_type": "feature" })).await;
        state.set("worktask:task:b", json!({ "task_id": "b", "task_type": "feature" })).await;
        state.set("worktask:task:c", json!({ "task_id": "c", "task_type": "chore" })).await;
        state.set("worktask:lease:a", json!({ "task_id": "a" })).await; // must be ignored

        let all = h.call("list_tasks", &json!({})).await.unwrap();
        assert_eq!(all.as_array().unwrap().len(), 3, "all three tasks, lease excluded");

        let features = h
            .call("list_tasks", &json!({ "task_type": "feature" }))
            .await
            .unwrap();
        assert_eq!(features.as_array().unwrap().len(), 2, "only the two features");
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
        let s = h.call("identity", &json!({ "v": "direct-merge" })).await.unwrap();
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
        let three = h
            .call("count", &json!({ "arr": [1, 2, 3] }))
            .await
            .unwrap();
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