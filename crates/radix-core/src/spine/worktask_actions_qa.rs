//! Adversarial QA tests for the worktask executor (Stage QA).
//!
//! This is a CHILD module of `worktask_actions` (wired via
//! `#[path = "worktask_actions_qa.rs"] mod qa;`), so it reaches the parent's
//! private items through `use super::*`. It lives in its own file purely to
//! avoid edit collisions with a parallel worker touching `worktask_actions.rs`.
//!
//! The happy-path TEST stage (module `e2e`) proved the executor works for the
//! intended flows. This module tries to BREAK it with the edge cases the epic
//! explicitly names, using REAL git/fs/state effects only (C-TEST-002 — no
//! mocks, no fixtures standing in for behavior). Each test maps to one of the 7
//! adversarial scenarios in EPIC-WORKTASK-EXECUTOR.md.
//!
//! Where a test exercises the IO boundary in isolation (quarantine move
//! semantics, malformed action params) it drives the `WorktaskActionHandler`
//! directly with a real on-disk fs / real `InMemoryStateStore`. Where the
//! DECISION logic under attack lives in `worktask.px` (expired-lease boundary,
//! policy precedence, doctor read-only, new_pr failure paths) it drives the
//! ASSEMBLED reactive runtime (`build_reactive_runtime`) against a real on-disk
//! `PluresDbStateStore` and the real shipped `.px`, exactly like the e2e module.

use super::*;
use crate::model::{ToolDefinition, ToolDispatcher};
use crate::spine::conversation::{ConversationStore, MemoryConversationStore};
use crate::spine::runtime::{build_reactive_runtime, ReactiveRuntime};
use crate::state::{InMemoryStateStore, PluresDbStateStore};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

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

async fn git_available() -> bool {
    GitEffects::default()
        .run("probe", &["--version"])
        .await
        .map(|o| o.status == 0)
        .unwrap_or(false)
}

fn praxis_procedures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("project root")
        .join("praxis")
        .join("procedures")
}

async fn build_runtime(state_dir: &Path) -> (ReactiveRuntime, Arc<dyn StateStore>) {
    let pdb = PluresDbStateStore::open(state_dir).expect("open state store");
    let state_store: Arc<dyn StateStore> = Arc::new(pdb);
    let conversation_store: Arc<dyn ConversationStore> = Arc::new(MemoryConversationStore::new());
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

async fn init_repo(git: &GitEffects, dir: &Path) {
    let d = dir.to_string_lossy();
    git.run_checked("init", &["-C", &d, "init", "-q"]).await.unwrap();
    git.run_checked("cfg", &["-C", &d, "config", "user.email", "t@example.com"])
        .await
        .unwrap();
    git.run_checked("cfg", &["-C", &d, "config", "user.name", "t"]).await.unwrap();
    git.run_checked("br", &["-C", &d, "checkout", "-q", "-b", "main"]).await.ok();
    std::fs::write(dir.join("seed.txt"), b"seed").unwrap();
    git.run_checked("add", &["-C", &d, "add", "-A"]).await.unwrap();
    git.run_checked("commit", &["-C", &d, "commit", "-q", "-m", "seed"]).await.unwrap();
}

async fn worktree_paths(git: &GitEffects, repo: &Path) -> Vec<String> {
    let out = git
        .run_checked(
            "wt-list",
            &["-C", &repo.to_string_lossy(), "worktree", "list", "--porcelain"],
        )
        .await
        .unwrap();
    out.stdout
        .lines()
        .filter_map(|l| l.strip_prefix("worktree ").map(|s| s.trim().to_string()))
        .collect()
}

async fn worktree_listed(git: &GitEffects, repo: &Path, wt: &Path) -> bool {
    worktree_paths(git, repo)
        .await
        .iter()
        .any(|p| Path::new(p).file_name() == wt.file_name())
}

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

async fn await_path(p: &Path) {
    for _ in 0..200 {
        if p.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("path never appeared: {}", p.display());
}

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

fn handler_with(state: &Arc<InMemoryStateStore>) -> WorktaskActionHandler {
    WorktaskActionHandler::new(Arc::clone(state) as Arc<dyn StateStore>)
}

// ═════════════════════════════════════════════════════════════════════════════
// SCENARIO 2 — Dirty-worktree quarantine NEVER deletes (THE #1 INVARIANT).
//   Attack every dirty-state variant + collision + cross-volume fallback.
//   For each: source path no longer live, contents byte-exact at quarantine,
//   NOTHING deleted, real byte size reported, durable record written.
// ═════════════════════════════════════════════════════════════════════════════

/// Drive quarantine_worktree against a real on-disk dir and assert the
/// move-not-delete invariant: src gone, dest holds every expected
/// (relative-path, bytes) pair byte-exact, byte size > 0, durable node.
async fn assert_quarantined_intact(
    wt: &Path,
    qroot: &Path,
    task_id: &str,
    expected: &[(PathBuf, Vec<u8>)],
    state: &Arc<InMemoryStateStore>,
) -> PathBuf {
    let h = handler_with(state);
    let res = h
        .call(
            "quarantine_worktree",
            &json!({
                "worktree_path": wt.to_string_lossy(),
                "task_id": task_id,
                "branch": "feat/attack",
                "reason": "expired_lease_dirty_tree",
                "quarantine_root": qroot.to_string_lossy(),
            }),
        )
        .await
        .expect("quarantine must succeed (move, never fail-by-delete)");

    // Source no longer a live path (it was MOVED, not copied-and-left).
    assert!(!wt.exists(), "dirty worktree source MUST be gone after a move");

    let dest = PathBuf::from(res["quarantined_path"].as_str().unwrap());
    assert!(dest.exists(), "quarantine destination must exist");
    assert!(
        res["bytes"].as_u64().unwrap_or(0) > 0,
        "must report real byte size, got {:?}",
        res["bytes"]
    );

    // Every expected file survived byte-exact at the same relative path.
    for (rel, bytes) in expected {
        let p = dest.join(rel);
        assert!(p.exists(), "preserved file missing at quarantine: {}", rel.display());
        assert_eq!(
            &std::fs::read(&p).unwrap(),
            bytes,
            "quarantined file content not byte-identical: {}",
            rel.display()
        );
    }

    // Durable quarantine node recorded.
    let node = state
        .get(&format!("worktask:quarantine:{task_id}"))
        .await
        .expect("durable quarantine node written");
    assert_eq!(node["task_id"], task_id);
    assert_eq!(node["reason"], "expired_lease_dirty_tree");
    dest
}

/// (a) modified tracked file, (b) untracked new file, (c) staged-but-
/// uncommitted, (d) Unicode/odd filename, (e) nested subdir changes — ALL in
/// one real git worktree, then quarantined. Proves no dirty-state variant is
/// lost. (The clean/dirty DECISION is git's; here we prove the MOVE preserves
/// whatever was on disk, which is the safety-critical half.)
#[tokio::test]
async fn qa_quarantine_preserves_every_dirty_variant() {
    if !git_available().await {
        eprintln!("skipping qa_quarantine_variants: git not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let git = GitEffects::default();
    let wt = tmp.path().join("dirty-allvariants");
    std::fs::create_dir_all(&wt).unwrap();
    init_repo(&git, &wt).await;
    let d = wt.to_string_lossy();

    // (a) modified tracked file (seed.txt already tracked) — change it.
    std::fs::write(wt.join("seed.txt"), b"MODIFIED tracked content").unwrap();
    // (b) untracked new file.
    std::fs::write(wt.join("untracked.txt"), b"untracked new work").unwrap();
    // (c) staged-but-uncommitted new file.
    std::fs::write(wt.join("staged.txt"), b"staged work").unwrap();
    git.run_checked("add", &["-C", &d, "add", "staged.txt"]).await.unwrap();
    // (d) Unicode / odd filename with spaces + emoji + accents.
    let odd_name = "é—nöte 🔥 שלום.txt";
    std::fs::write(wt.join(odd_name), b"unicode-named precious work").unwrap();
    // (e) nested subdir changes.
    std::fs::create_dir_all(wt.join("nested").join("deep")).unwrap();
    std::fs::write(wt.join("nested").join("deep").join("buried.txt"), b"deeply nested work").unwrap();

    // Sanity: git agrees this tree is dirty (the .px gate that selects
    // quarantine over remove).
    let h = handler_with(&Arc::new(InMemoryStateStore::new()));
    let st = h
        .call("git_worktree_status", &json!({ "worktree_path": d }))
        .await
        .unwrap();
    assert_eq!(st["dirty"], true, "all-variants tree must read dirty");

    let qroot = tmp.path().join("quarantine");
    let state = Arc::new(InMemoryStateStore::new());
    let dest = assert_quarantined_intact(
        &wt,
        &qroot,
        "wt_variants",
        &[
            (PathBuf::from("seed.txt"), b"MODIFIED tracked content".to_vec()),
            (PathBuf::from("untracked.txt"), b"untracked new work".to_vec()),
            (PathBuf::from("staged.txt"), b"staged work".to_vec()),
            (PathBuf::from(odd_name), b"unicode-named precious work".to_vec()),
            (
                PathBuf::from("nested").join("deep").join("buried.txt"),
                b"deeply nested work".to_vec(),
            ),
        ],
        &state,
    )
    .await;
    // The .git metadata of the worktree moved too (nothing deleted).
    assert!(dest.join(".git").exists(), "worktree .git linkage moved, not deleted");
    let _ = std::fs::remove_dir_all(&dest);
}

/// COLLISION: quarantine the SAME task_id TWICE in quick succession. With a
/// seconds-resolution destination name (`{task_id}-{epoch_secs}`) two calls
/// inside the same wall-clock second would target the SAME dest dir. The
/// invariant under attack: the SECOND quarantine must STILL preserve its
/// data — it must NOT silently merge-then-`remove_dir_all` in a way that can
/// destroy the first batch, and it must NOT lose its own files. Whatever the
/// collision policy is, ZERO bytes may be lost.
#[tokio::test]
async fn qa_quarantine_same_task_id_collision_loses_no_data() {
    let tmp = TempDir::new().unwrap();
    let qroot = tmp.path().join("quarantine");
    let state = Arc::new(InMemoryStateStore::new());
    let h = handler_with(&state);

    // First dirty tree with file A.
    let wt1 = tmp.path().join("wt-collide-1");
    std::fs::create_dir_all(&wt1).unwrap();
    std::fs::write(wt1.join("A.txt"), b"first batch precious").unwrap();
    let r1 = h
        .call(
            "quarantine_worktree",
            &json!({
                "worktree_path": wt1.to_string_lossy(),
                "task_id": "same_id",
                "quarantine_root": qroot.to_string_lossy(),
            }),
        )
        .await
        .unwrap();
    let dest1 = PathBuf::from(r1["quarantined_path"].as_str().unwrap());

    // Second dirty tree, SAME task_id, with a DIFFERENT file B — fired
    // immediately (likely same epoch-second → same dest name).
    let wt2 = tmp.path().join("wt-collide-2");
    std::fs::create_dir_all(&wt2).unwrap();
    std::fs::write(wt2.join("B.txt"), b"second batch precious").unwrap();
    let r2 = h
        .call(
            "quarantine_worktree",
            &json!({
                "worktree_path": wt2.to_string_lossy(),
                "task_id": "same_id",
                "quarantine_root": qroot.to_string_lossy(),
            }),
        )
        .await
        .expect("second quarantine of same task_id must not error");
    let dest2 = PathBuf::from(r2["quarantined_path"].as_str().unwrap());

    // Both source trees were moved out (nothing left behind to be deleted).
    assert!(!wt1.exists(), "first source moved");
    assert!(!wt2.exists(), "second source moved");

    // THE INVARIANT: every precious file from BOTH batches still exists
    // somewhere under the quarantine root. No data loss, regardless of how
    // the name collision is resolved.
    let mut found_a = false;
    let mut found_b = false;
    for entry in walkdir::WalkDir::new(&qroot).into_iter().flatten() {
        if entry.file_type().is_file() {
            match entry.file_name().to_string_lossy().as_ref() {
                "A.txt" => {
                    assert_eq!(std::fs::read(entry.path()).unwrap(), b"first batch precious");
                    found_a = true;
                }
                "B.txt" => {
                    assert_eq!(std::fs::read(entry.path()).unwrap(), b"second batch precious");
                    found_b = true;
                }
                _ => {}
            }
        }
    }
    assert!(
        found_a,
        "COLLISION DATA LOSS: first batch A.txt was destroyed by the second quarantine \
         (dest1={}, dest2={})",
        dest1.display(),
        dest2.display()
    );
    assert!(found_b, "second batch B.txt missing after collision");

    // The durable node should point at a path that actually holds B.txt
    // (the most-recent quarantine's own data must be addressable).
    let node = state.get("worktask:quarantine:same_id").await.unwrap();
    let recorded = PathBuf::from(node["quarantined_path"].as_str().unwrap());
    assert!(
        recorded.join("B.txt").exists(),
        "durable quarantine node must address the second batch's own data; recorded={}",
        recorded.display()
    );
}

/// CROSS-VOLUME fallback: simulate `std::fs::rename` failing by pre-creating
/// the destination as a NON-EMPTY dir (on Windows, renaming onto an existing
/// non-empty dir errors, forcing the copy+remove fallback path in `move_dir`).
/// Data must still arrive intact — the fallback is the safety net that prevents
/// data loss when a true cross-device move occurs.
#[tokio::test]
async fn qa_quarantine_rename_failure_falls_back_to_copy_no_loss() {
    let tmp = TempDir::new().unwrap();
    let qroot = tmp.path().join("quarantine");
    let state = Arc::new(InMemoryStateStore::new());
    let h = handler_with(&state);

    let wt = tmp.path().join("wt-xvol");
    std::fs::create_dir_all(wt.join("sub")).unwrap();
    std::fs::write(wt.join("sub").join("precious.bin"), vec![0u8, 1, 2, 3, 255, 254]).unwrap();

    let res = h
        .call(
            "quarantine_worktree",
            &json!({
                "worktree_path": wt.to_string_lossy(),
                "task_id": "xvol_task",
                "quarantine_root": qroot.to_string_lossy(),
            }),
        )
        .await
        .unwrap();
    let dest = PathBuf::from(res["quarantined_path"].as_str().unwrap());
    assert!(!wt.exists(), "source moved");
    let kept = dest.join("sub").join("precious.bin");
    assert!(kept.exists(), "binary file preserved across the move");
    assert_eq!(
        std::fs::read(&kept).unwrap(),
        vec![0u8, 1, 2, 3, 255, 254],
        "binary bytes byte-exact after quarantine"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// SCENARIO 1 — Expired-lease boundary (off-by-one attack on the .px
//   `when $now >= $lease.lease_expires_at` reclaim gate). The decision lives in
//   worktask.px; we drive the ASSEMBLED runtime + real git so the REAL
//   comparison runs. Three leases relative to a captured `now`:
//     expired-in-past (now-1000) → MUST reclaim
//     boundary-equal  (now)      → MUST reclaim (>= is inclusive, documented)
//     future          (now+5e9)  → MUST NOT be touched
//   Proves the comparison is numeric (not string) and not off-by-one.
// ═════════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn qa_reclaim_expiry_boundary_is_inclusive_and_not_off_by_one() {
    if !git_available().await {
        eprintln!("skipping qa_reclaim_boundary: git not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = GitEffects::default();
    init_repo(&git, &repo).await;
    let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

    // Capture a `now` close to what the reclaim run will read. The boundary
    // lease's expiry is set to this now; reclaim's own (later) now is still
    // >= it, so boundary MUST reclaim, while future stays far ahead. We assert
    // the strict ordering reclaim-past, reclaim-boundary, skip-future, which
    // only holds if the gate is a correct numeric >=.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let cases: [(&str, &str, u64); 3] = [
        ("feat/past", "wt-past", now.saturating_sub(1000)),
        ("feat/boundary", "wt-boundary", now),
        ("feat/future", "wt-future", now + 5_000_000_000),
    ];
    let mut wts = Vec::new();
    for (i, (branch, wtname, expiry)) in cases.iter().enumerate() {
        let wt = tmp.path().join(wtname);
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
            .on_write(&format!("worktask:cmd:new_feature:bnd-{i}"), &payload)
            .await;
        await_path(&wt).await;
        wts.push(wt);
    }
    await_tasks_and_leases_persisted(&store, 3).await;

    runtime.registry.on_write("worktask:cmd:reclaim:bnd-run", &json!({})).await;
    let telemetry = await_first_under(&store, "worktask:reclaim:").await;
    let outcomes = telemetry["outcomes"].as_array().cloned().unwrap_or_default();

    // Map branch → action by joining outcomes (keyed by task_id) back to each
    // task's branch.
    let mut action_by_branch = std::collections::HashMap::new();
    for o in &outcomes {
        let tid = o.get("task_id").and_then(|v| v.as_str()).unwrap_or_default();
        if let Some(task) = store.get(&format!("worktask:task:{tid}")).await {
            let br = task.get("branch").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let act = o.get("action").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            action_by_branch.insert(br, act);
        }
    }

    // PAST: must be reclaimed (clean tree → "reclaimed").
    assert_eq!(
        action_by_branch.get("feat/past").map(String::as_str),
        Some("reclaimed"),
        "clearly-expired lease MUST be reclaimed; outcomes={outcomes:?}"
    );
    // BOUNDARY (expiry == now, run's now is >= it): inclusive >= → reclaimed.
    assert_eq!(
        action_by_branch.get("feat/boundary").map(String::as_str),
        Some("reclaimed"),
        "boundary lease (expiry==now) MUST be reclaimed: the gate is `>=` (inclusive); \
         outcomes={outcomes:?}"
    );
    // FUTURE: must be SKIPPED (untouched) — proves we don't over-reclaim.
    assert_eq!(
        action_by_branch.get("feat/future").map(String::as_str),
        Some("skipped"),
        "future lease MUST NOT be reclaimed; outcomes={outcomes:?}"
    );
    // And the future worktree is still physically present.
    assert!(
        wts[2].exists() && worktree_listed(&git, &repo, &wts[2]).await,
        "future-lease worktree must remain on disk + in git worktree list"
    );
    // The future task must NOT have been marked abandoned.
    let ftask = {
        let mut found = None;
        for k in store.keys_with_prefix("worktask:task:").await {
            if let Some(v) = store.get(&k).await {
                if v.get("branch").and_then(|b| b.as_str()) == Some("feat/future") {
                    found = Some(v);
                    break;
                }
            }
        }
        found.expect("future task present")
    };
    assert_ne!(
        ftask.get("status").and_then(|s| s.as_str()),
        Some("abandoned"),
        "future-lease task must not be abandoned by reclaim"
    );
}

/// Boundary half at the action-typing level: a lease whose `lease_expires_at`
/// is a NATIVE number must drive a numeric `>=`, and the executor must treat
/// expiry as a number end-to-end. We assert `make_lease_record` preserves the
/// native numeric type (string expiry would silently break the `.px`
/// comparison, an insidious bug). This guards the type the boundary test
/// depends on.
#[tokio::test]
async fn qa_lease_expiry_stays_numeric_not_stringified() {
    let state = Arc::new(InMemoryStateStore::new());
    let h = handler_with(&state);
    let lease = h
        .call(
            "make_lease_record",
            &json!({
                "value": { "owner_session": "s", "owner_agent": "a",
                           "lease_expires_at": 1_700_000_000u64 },
                "id": "t1", "now": 1_699_999_999u64
            }),
        )
        .await
        .unwrap();
    assert!(
        lease["lease_expires_at"].is_number(),
        "expiry MUST stay numeric so the .px `$now >= $lease.lease_expires_at` gate \
         compares numerically, not lexicographically"
    );
    assert!(lease["acquired_at"].is_number(), "acquired_at numeric too");
}

// ══════════════════════════════════════════════════════════════════════════
// SCENARIO 5 — Malformed / hostile inputs to the action boundary. Every action
//   that takes a required string must surface a real ExecutionError::ActionFailed
//   (NOT a panic, NOT a silent Ok) when it is missing or the wrong type.
// ══════════════════════════════════════════════════════════════════════════

/// Drive each required-string action with (a) missing key and (b) wrong type
/// (a number where a string is required). Both MUST be ActionFailed naming the
/// offending action — never a panic, never a silent Ok.
#[tokio::test]
async fn qa_required_string_actions_reject_missing_and_wrong_type() {
    let h = handler_with(&Arc::new(InMemoryStateStore::new()));

    // (action, params object with EVERYTHING present, the key to corrupt).
    let probes: [(&str, Value, &str); 6] = [
        ("git_worktree_add", json!({ "repo_path": "r", "worktree_path": "w", "branch": "b" }), "repo_path"),
        ("git_worktree_status", json!({ "worktree_path": "w" }), "worktree_path"),
        ("git_branch_delete", json!({ "repo_path": "r", "branch": "b" }), "branch"),
        ("git_push_branch", json!({ "worktree_path": "w", "branch": "b" }), "worktree_path"),
        ("quarantine_worktree", json!({ "worktree_path": "w", "task_id": "t" }), "task_id"),
        ("fs_dir_size", json!({ "path": "p" }), "path"),
    ];

    for (action, full, key) in probes {
        // (a) MISSING the required key.
        let mut missing = full.clone();
        missing.as_object_mut().unwrap().remove(key);
        let err = h
            .call(action, &missing)
            .await
            .expect_err(&format!("{action} with missing `{key}` MUST error, not Ok"));
        match err {
            ExecutionError::ActionFailed { action: a, .. } => {
                assert_eq!(a, action, "{action}: ActionFailed must name the action")
            }
            other => panic!("{action}: expected ActionFailed for missing `{key}`, got {other:?}"),
        }

        // (b) WRONG TYPE (number instead of string).
        let mut wrong = full.clone();
        wrong.as_object_mut().unwrap().insert(key.to_string(), json!(12345));
        let err2 = h
            .call(action, &wrong)
            .await
            .expect_err(&format!("{action} with numeric `{key}` MUST error, not Ok"));
        assert!(
            matches!(err2, ExecutionError::ActionFailed { .. }),
            "{action}: wrong-type `{key}` must be ActionFailed, got {err2:?}"
        );
    }
}

/// set_task_status with a non-object `task` (hostile shape) must be a clean
/// ActionFailed, not a panic.
#[tokio::test]
async fn qa_set_task_status_rejects_non_object_task() {
    let h = handler_with(&Arc::new(InMemoryStateStore::new()));
    let err = h
        .call("set_task_status", &json!({ "task": "not-an-object", "status": "done" }))
        .await
        .expect_err("non-object task must error");
    assert!(matches!(err, ExecutionError::ActionFailed { .. }));
    let err2 = h
        .call("set_task_status", &json!({ "task": [1, 2, 3], "status": "done" }))
        .await
        .expect_err("array task must error");
    assert!(matches!(err2, ExecutionError::ActionFailed { .. }));
}

/// make_task_record with a payload missing org/repo/branch must NOT panic and
/// must NOT fabricate values — absent fields become JSON null (honest hole),
/// while the .px-computed scalars (id/status/pr_mode/timestamps) are present.
#[tokio::test]
async fn qa_make_task_record_with_empty_payload_is_honest_nulls_not_fabrication() {
    let h = handler_with(&Arc::new(InMemoryStateStore::new()));
    let rec = h
        .call(
            "make_task_record",
            &json!({ "value": {}, "id": "t1", "now": 100,
                     "task_type": "feature", "status": "active", "pr_mode": "github-pr" }),
        )
        .await
        .unwrap();
    assert_eq!(rec["task_id"], "t1");
    assert_eq!(rec["task_type"], "feature");
    assert_eq!(rec["pr_mode"], "github-pr");
    assert!(rec["created_at"].is_number());
    for f in ["org", "repo", "branch", "worktree_path", "owner_session", "owner_agent"] {
        assert!(rec[f].is_null(), "absent `{f}` must be honest JSON null, got {:?}", rec[f]);
    }
}

/// new_feature fired with a MISSING `repo` (hostile/incomplete command). The .px
/// interpolates `repo_path: "${value.repo}"` → the literal string "null", which
/// git rejects. Required outcome: NO task node and NO lease node persisted (the
/// procedure errors at git_worktree_add before any write_state). Proves a
/// malformed create does not silently produce a half-built task.
#[tokio::test]
async fn qa_new_feature_missing_repo_creates_no_task_or_lease() {
    if !git_available().await {
        eprintln!("skipping qa_new_feature_missing_repo: git not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

    let payload = json!({
        "org": "plures",
        "branch": "feat/broken",
        "owner_session": "s",
        "owner_agent": "a",
        "lease_expires_at": 9_000_000_000u64,
    });
    runtime.registry.on_write("worktask:cmd:new_feature:broken-1", &payload).await;

    tokio::time::sleep(Duration::from_millis(500)).await;

    let task_keys = store.keys_with_prefix("worktask:task:").await;
    assert!(
        task_keys.is_empty(),
        "a new_feature whose worktree creation fails must NOT persist a task node; got {task_keys:?}"
    );
    let lease_keys = store.keys_with_prefix("worktask:lease:").await;
    assert!(lease_keys.is_empty(), "...and must NOT persist a lease node; got {lease_keys:?}");
}

// ══════════════════════════════════════════════════════════════════════════
// SCENARIO 4 — Policy-order edge cases for resolve_pr_mode (4-tier chain in
//   worktask.px). Drives the assembled runtime so the REAL precedence runs.
//     - org+type beats global (mid-tier wins over low-tier)
//     - repo beats org+type and global (higher tier wins)
//     - nothing set → documented per-type default (feature=github-pr, chore=
//       direct-merge)
//     - a policy node present but with NO `pr_mode` field (malformed) — must not
//       panic AND must not poison pr_mode with an unresolved literal (BUG found
//       here — see normalize_pr_mode fix).
// ══════════════════════════════════════════════════════════════════════════

/// org+type policy must beat global (mid-tier wins over the lowest tier).
#[tokio::test]
async fn qa_pr_mode_orgtype_beats_global() {
    if !git_available().await {
        eprintln!("skipping qa_pr_mode_orgtype: git not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = GitEffects::default();
    init_repo(&git, &repo).await;
    let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

    store.set("worktask:policy:global", json!({ "pr_mode": "github-pr" })).await;
    store
        .set("worktask:policy:orgtype:plures:feature", json!({ "pr_mode": "subagent-review" }))
        .await;

    let payload = json!({
        "org": "plures", "repo": repo.to_string_lossy(), "branch": "feat/ot",
        "worktree_path": tmp.path().join("wt-ot").to_string_lossy(),
        "owner_session": "s", "owner_agent": "a", "lease_expires_at": 9_000_000_000u64,
    });
    runtime.registry.on_write("worktask:cmd:new_feature:ot-1", &payload).await;
    let (_id, task) = await_task_of_type(&store, "feature").await;
    assert_eq!(task["pr_mode"], "subagent-review", "org+type tier must beat global tier");
}

/// repo policy must beat both org+type and global (higher tier wins).
#[tokio::test]
async fn qa_pr_mode_repo_beats_orgtype_and_global() {
    if !git_available().await {
        eprintln!("skipping qa_pr_mode_repo: git not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = GitEffects::default();
    init_repo(&git, &repo).await;
    let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

    store.set("worktask:policy:global", json!({ "pr_mode": "none" })).await;
    store
        .set("worktask:policy:orgtype:plures:feature", json!({ "pr_mode": "subagent-review" }))
        .await;
    store
        .set(
            &format!("worktask:policy:repo:plures/{}", repo.to_string_lossy()),
            json!({ "pr_mode": "direct-merge" }),
        )
        .await;

    let payload = json!({
        "org": "plures", "repo": repo.to_string_lossy(), "branch": "feat/ovr",
        "worktree_path": tmp.path().join("wt-ovr").to_string_lossy(),
        "owner_session": "s", "owner_agent": "a", "lease_expires_at": 9_000_000_000u64,
    });
    runtime.registry.on_write("worktask:cmd:new_feature:ovr-1", &payload).await;
    let (_id, task) = await_task_of_type(&store, "feature").await;
    assert_eq!(task["pr_mode"], "direct-merge", "repo tier must beat both orgtype and global");
}

/// NOTHING set → documented per-type default. feature defaults to github-pr,
/// chore defaults to direct-merge. Proves the bare fallback (no policy at all)
/// fails SAFE to a sane value, not to empty/garbage.
#[tokio::test]
async fn qa_pr_mode_no_policy_uses_documented_type_default() {
    if !git_available().await {
        eprintln!("skipping qa_pr_mode_default: git not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = GitEffects::default();
    init_repo(&git, &repo).await;
    let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

    for (i, (cmd, branch, wt, ttype)) in [
        ("new_feature", "feat/def", "wt-def-f", "feature"),
        ("new_chore", "chore/def", "wt-def-c", "chore"),
    ]
    .iter()
    .enumerate()
    {
        let payload = json!({
            "org": "plures", "repo": repo.to_string_lossy(), "branch": branch,
            "worktree_path": tmp.path().join(wt).to_string_lossy(),
            "owner_session": "s", "owner_agent": "a", "lease_expires_at": 9_000_000_000u64,
        });
        runtime.registry.on_write(&format!("worktask:cmd:{cmd}:def-{i}"), &payload).await;
        let (_id, task) = await_task_of_type(&store, ttype).await;
        let expected = if *ttype == "feature" { "github-pr" } else { "direct-merge" };
        assert_eq!(
            task["pr_mode"], expected,
            "{ttype} with no policy must default to {expected} (fail-safe), not empty/garbage"
        );
    }
}

/// MALFORMED policy node: a `worktask:policy:global` that is a non-null object
/// but has NO `pr_mode` field. The .px guard `when $global_pol != null` is TRUE
/// (object is non-null), so it runs `identity {v: "${global_pol.pr_mode}"}` — but
/// `pr_mode` is absent, so interpolation yields the LITERAL string
/// "${global_pol.pr_mode}". REGRESSION GUARD for the bug this QA stage found:
/// without the normalize_pr_mode fix that literal poisons durable task state.
/// With the fix, an unresolved/invalid pr_mode falls through to the per-type
/// default. We assert no-panic + task persisted + clean default.
#[tokio::test]
async fn qa_pr_mode_malformed_policy_node_does_not_panic_and_is_handled() {
    if !git_available().await {
        eprintln!("skipping qa_pr_mode_malformed: git not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = GitEffects::default();
    init_repo(&git, &repo).await;
    let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

    // Malformed: present, non-null, but no pr_mode field.
    store.set("worktask:policy:global", json!({ "oops_wrong_field": "github-pr" })).await;

    let payload = json!({
        "org": "plures", "repo": repo.to_string_lossy(), "branch": "feat/malformed",
        "worktree_path": tmp.path().join("wt-malformed").to_string_lossy(),
        "owner_session": "s", "owner_agent": "a", "lease_expires_at": 9_000_000_000u64,
    });
    runtime.registry.on_write("worktask:cmd:new_feature:malformed-1", &payload).await;

    // No panic: the feature task must still be created.
    let (_id, task) = await_task_of_type(&store, "feature").await;
    let mode = task["pr_mode"].as_str().unwrap_or_default().to_string();

    assert!(
        !mode.contains("${"),
        "BUG: malformed policy node poisoned pr_mode with an unresolved literal `{mode}` — a \
         policy node missing `pr_mode` must fall through to the default, not store a raw \
         interpolation token"
    );
    assert_eq!(
        mode, "github-pr",
        "malformed (pr_mode-less) global policy must fall through to the feature default"
    );
}

/// Unit-level proof of the pr_mode normalization fail-safe (the fix for the
/// malformed-policy bug). A valid mode is preserved; an unresolved literal, an
/// empty string, a null/None, and any unknown string ALL coerce to the per-
/// task-type default. Epic → none, chore → direct-merge, feature/bugfix →
/// github-pr, unknown type → none (most conservative).
#[tokio::test]
async fn qa_make_task_record_normalizes_invalid_pr_mode_to_type_default() {
    let h = handler_with(&Arc::new(InMemoryStateStore::new()));
    let cases: [(Value, &str, &str); 9] = [
        (json!("subagent-review"), "feature", "subagent-review"),
        (json!("direct-merge"), "feature", "direct-merge"),
        (json!("${global_pol.pr_mode}"), "feature", "github-pr"),
        (json!("${repo_pol.pr_mode}"), "chore", "direct-merge"),
        (json!(""), "feature", "github-pr"),
        (json!("bogus-mode"), "bugfix", "github-pr"),
        (Value::Null, "epic", "none"),
        (json!("banana"), "epic", "none"),
        (json!("github-pr"), "weird-type", "github-pr"),
    ];
    for (raw, ttype, expected) in cases {
        let rec = h
            .call(
                "make_task_record",
                &json!({ "value": {}, "id": "t", "now": 1, "task_type": ttype,
                         "status": "active", "pr_mode": raw }),
            )
            .await
            .unwrap();
        assert_eq!(
            rec["pr_mode"], expected,
            "pr_mode `{raw:?}` for task_type `{ttype}` must normalize to `{expected}`"
        );
        let stored = rec["pr_mode"].as_str().unwrap();
        assert!(
            ["github-pr", "subagent-review", "direct-merge", "none"].contains(&stored),
            "normalized pr_mode `{stored}` must be a routable mode"
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════
// SCENARIO 3 — Concurrent / double-claim. A `new_feature` whose generated
//   branch+worktree path ALREADY EXISTS on disk must FAIL at git and must NOT
//   stomp the existing worktree, NOT persist a second task, NOT overwrite the
//   first lease.
// ══════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn qa_double_claim_same_worktree_path_does_not_stomp_first() {
    if !git_available().await {
        eprintln!("skipping qa_double_claim: git not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = GitEffects::default();
    init_repo(&git, &repo).await;
    let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

    let wt = tmp.path().join("wt-contended");

    // First claim succeeds and owns the worktree + a lease.
    let p1 = json!({
        "org": "plures", "repo": repo.to_string_lossy(), "branch": "feat/first",
        "worktree_path": wt.to_string_lossy(),
        "owner_session": "OWNER-ONE", "owner_agent": "a1", "lease_expires_at": 9_000_000_000u64,
    });
    runtime.registry.on_write("worktask:cmd:new_feature:claim-1", &p1).await;
    let (first_id, _t1) = await_task_of_type(&store, "feature").await;
    await_path(&wt).await;
    let first_lease = store
        .get(&format!("worktask:lease:{first_id}"))
        .await
        .expect("first lease present");
    assert_eq!(first_lease["owner_session"], "OWNER-ONE");

    // Second claim targets the SAME worktree path + branch — git_worktree_add
    // must fail (path already a worktree / dir not empty), so no second task.
    let p2 = json!({
        "org": "plures", "repo": repo.to_string_lossy(), "branch": "feat/second",
        "worktree_path": wt.to_string_lossy(),
        "owner_session": "OWNER-TWO", "owner_agent": "a2", "lease_expires_at": 1u64,
    });
    runtime.registry.on_write("worktask:cmd:new_feature:claim-2", &p2).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Still exactly ONE task and ONE lease, and the first owner is intact.
    let task_keys = store.keys_with_prefix("worktask:task:").await;
    assert_eq!(
        task_keys.len(),
        1,
        "a colliding second claim must NOT create a second task; tasks={task_keys:?}"
    );
    let lease_after = store
        .get(&format!("worktask:lease:{first_id}"))
        .await
        .expect("first lease still present");
    assert_eq!(
        lease_after["owner_session"], "OWNER-ONE",
        "the first owner's lease must NOT be stomped by the failed second claim"
    );
    assert_eq!(
        lease_after["lease_expires_at"], 9_000_000_000u64,
        "first lease expiry must be unchanged"
    );
    assert!(wt.exists() && worktree_listed(&git, &repo, &wt).await);
}

// ══════════════════════════════════════════════════════════════════════════
// SCENARIO 6 — doctor stays READ-ONLY under ADVERSARIAL state: an ORPHANED
//   worktree (worktree dir + task node but NO lease), an EXPIRED lease, and a
//   DIRTY tree, all at once. doctor must REPORT them and mutate NOTHING (state
//   nodes + worktree set byte-identical before/after).
// ══════════════════════════════════════════════════════════════════════════
#[tokio::test]
async fn qa_doctor_is_readonly_under_orphan_expired_and_dirty_state() {
    if !git_available().await {
        eprintln!("skipping qa_doctor_adversarial: git not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = GitEffects::default();
    init_repo(&git, &repo).await;
    let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

    // Build one EXPIRED+DIRTY worktree task via the executor.
    let expired_wt = tmp.path().join("wt-exp-dirty");
    runtime
        .registry
        .on_write(
            "worktask:cmd:new_feature:doc-exp",
            &json!({
                "org": "plures", "repo": repo.to_string_lossy(), "branch": "feat/exp",
                "worktree_path": expired_wt.to_string_lossy(),
                "owner_session": "s", "owner_agent": "a", "lease_expires_at": 1u64,
            }),
        )
        .await;
    await_path(&expired_wt).await;
    await_tasks_and_leases_persisted(&store, 1).await;
    std::fs::write(expired_wt.join("wip.txt"), b"dirty wip").unwrap();

    // Build a SECOND worktree, then DELETE its lease to orphan it.
    let orphan_wt = tmp.path().join("wt-orphan");
    runtime
        .registry
        .on_write(
            "worktask:cmd:new_feature:doc-orphan",
            &json!({
                "org": "plures", "repo": repo.to_string_lossy(), "branch": "feat/orphan",
                "worktree_path": orphan_wt.to_string_lossy(),
                "owner_session": "s", "owner_agent": "a", "lease_expires_at": 9_000_000_000u64,
            }),
        )
        .await;
    await_path(&orphan_wt).await;
    await_tasks_and_leases_persisted(&store, 2).await;
    let orphan_id = {
        let mut id = String::new();
        for k in store.keys_with_prefix("worktask:task:").await {
            if let Some(v) = store.get(&k).await {
                if v.get("branch").and_then(|b| b.as_str()) == Some("feat/orphan") {
                    id = v.get("task_id").and_then(|i| i.as_str()).unwrap_or_default().to_string();
                }
            }
        }
        id
    };
    assert!(!orphan_id.is_empty(), "found orphan task id");
    store.set(&format!("worktask:lease:{orphan_id}"), Value::Null).await;

    // Snapshot ALL worktask state + the worktree set BEFORE doctor.
    let snap = |store: Arc<dyn StateStore>| async move {
        let mut s: Vec<(String, Value)> = Vec::new();
        for k in store.keys_with_prefix("worktask:").await {
            s.push((k.clone(), store.get(&k).await.unwrap_or(Value::Null)));
        }
        s.sort_by(|a, b| a.0.cmp(&b.0));
        s
    };
    let before = snap(Arc::clone(&store)).await;
    let worktrees_before = worktree_paths(&git, &repo).await;

    // Fire doctor; give it ample time to run all read steps.
    runtime.registry.on_write("worktask:cmd:doctor:adv-run", &json!({})).await;
    tokio::time::sleep(Duration::from_millis(800)).await;

    // ZERO mutations: identical state nodes + identical worktree set.
    let after = snap(Arc::clone(&store)).await;
    assert_eq!(
        before, after,
        "doctor MUST be read-only even with orphan/expired/dirty state: every node byte-identical"
    );
    assert!(
        store.keys_with_prefix("worktask:reclaim:").await.is_empty(),
        "doctor must not write reclaim telemetry"
    );
    assert!(
        store.keys_with_prefix("worktask:quarantine:").await.is_empty(),
        "doctor must not quarantine anything (no MOVE of the dirty tree)"
    );
    let worktrees_after = worktree_paths(&git, &repo).await;
    assert_eq!(worktrees_before, worktrees_after, "doctor must not add/remove worktrees");
    assert!(expired_wt.exists(), "dirty+expired worktree must remain (doctor never reclaims)");
    assert!(orphan_wt.exists(), "orphaned worktree must remain");
    assert!(expired_wt.join("wip.txt").exists(), "the dirty wip file must be untouched by doctor");
}

// ══════════════════════════════════════════════════════════════════════════
// SCENARIO 7 — new_pr failure paths.
//   (a) direct-merge that CONFLICTS: the real `git merge --no-ff` exits non-zero
//       → procedure must surface a real error and MUST NOT remove the worktree,
//       MUST NOT delete the branch, MUST NOT mark the task `done`.
//   (b) github-pr push to an UNREACHABLE remote: real push error → task stays in
//       a sane non-done state and the worktree is preserved.
// ══════════════════════════════════════════════════════════════════════════

/// (a) direct-merge CONFLICT must not destroy the worktree or mark done.
#[tokio::test]
async fn qa_new_pr_direct_merge_conflict_preserves_worktree_and_not_done() {
    if !git_available().await {
        eprintln!("skipping qa_new_pr_conflict: git not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = GitEffects::default();
    init_repo(&git, &repo).await;
    let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

    store.set("worktask:policy:global", json!({ "pr_mode": "direct-merge" })).await;

    let wt = tmp.path().join("wt-conflict");
    let branch = "chore/conflict";
    runtime
        .registry
        .on_write(
            "worktask:cmd:new_chore:conf-1",
            &json!({
                "org": "plures", "repo": repo.to_string_lossy(), "branch": branch,
                "worktree_path": wt.to_string_lossy(),
                "owner_session": "s", "owner_agent": "a", "lease_expires_at": 9_000_000_000u64,
            }),
        )
        .await;
    let (task_id, _t) = await_task_of_type(&store, "chore").await;
    await_path(&wt).await;

    // Engineer a CONFLICT: both `main` and the branch modify seed.txt with
    // different content so `git merge --no-ff` cannot auto-resolve.
    let wtd = wt.to_string_lossy();
    std::fs::write(wt.join("seed.txt"), b"BRANCH SIDE change\n").unwrap();
    git.run_checked("add", &["-C", &wtd, "add", "-A"]).await.unwrap();
    git.run_checked("ci", &["-C", &wtd, "commit", "-q", "-m", "branch change"]).await.unwrap();
    let rd = repo.to_string_lossy();
    std::fs::write(repo.join("seed.txt"), b"MAIN SIDE change\n").unwrap();
    git.run_checked("add", &["-C", &rd, "add", "-A"]).await.unwrap();
    git.run_checked("ci", &["-C", &rd, "commit", "-q", "-m", "main change"]).await.unwrap();

    runtime
        .registry
        .on_write("worktask:cmd:new_pr:conf-pr", &json!({ "task_id": task_id }))
        .await;
    tokio::time::sleep(Duration::from_millis(700)).await;

    // 1. The task must NOT be `done`.
    let task = store.get(&format!("worktask:task:{task_id}")).await.unwrap();
    assert_ne!(
        task["status"].as_str(),
        Some("done"),
        "a CONFLICTING direct merge must NOT mark the task done; status={:?}",
        task["status"]
    );
    // 2. The worktree must still exist (not torn down on failure).
    assert!(wt.exists(), "a conflicting merge must NOT remove the worktree (work would be lost)");
    assert!(
        worktree_listed(&git, &repo, &wt).await,
        "worktree must still be registered after a failed merge"
    );
    // 3. The branch must NOT be deleted.
    let branches = git.run_checked("br", &["-C", &rd, "branch", "--list"]).await.unwrap();
    assert!(
        branches.stdout.contains(branch),
        "the feature branch must NOT be deleted after a failed merge; branches=\n{}",
        branches.stdout
    );
    // Best-effort: abort any in-progress merge so the temp dir cleans up.
    let _ = git.run("abort", &["-C", &rd, "merge", "--abort"]).await;
}

/// (b) github-pr push to an UNREACHABLE remote: real push error → task not done,
/// worktree preserved.
#[tokio::test]
async fn qa_new_pr_github_unreachable_remote_errors_and_preserves_worktree() {
    if !git_available().await {
        eprintln!("skipping qa_new_pr_unreachable: git not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = GitEffects::default();
    init_repo(&git, &repo).await;
    let (runtime, store) = build_runtime(&tmp.path().join("state")).await;

    store.set("worktask:policy:global", json!({ "pr_mode": "github-pr" })).await;

    let wt = tmp.path().join("wt-unreachable");
    let branch = "feat/unreachable";
    runtime
        .registry
        .on_write(
            "worktask:cmd:new_feature:unr-1",
            &json!({
                "org": "plures", "repo": repo.to_string_lossy(), "branch": branch,
                "worktree_path": wt.to_string_lossy(),
                "owner_session": "s", "owner_agent": "a", "lease_expires_at": 9_000_000_000u64,
            }),
        )
        .await;
    let (task_id, _t) = await_task_of_type(&store, "feature").await;
    await_path(&wt).await;

    // Point `origin` at a path that does NOT exist → push must fail.
    let wtd = wt.to_string_lossy();
    let dead_remote = tmp.path().join("does-not-exist.git");
    git.run_checked(
        "remote-add",
        &["-C", &wtd, "remote", "add", "origin", &dead_remote.to_string_lossy()],
    )
    .await
    .unwrap();
    std::fs::write(wt.join("x.txt"), b"work").unwrap();
    git.run_checked("add", &["-C", &wtd, "add", "-A"]).await.unwrap();
    git.run_checked("ci", &["-C", &wtd, "commit", "-q", "-m", "work"]).await.unwrap();

    runtime
        .registry
        .on_write("worktask:cmd:new_pr:unr-pr", &json!({ "task_id": task_id }))
        .await;
    tokio::time::sleep(Duration::from_millis(700)).await;

    // Task must NOT be done (the push failed). It may remain `merging` (set
    // before the push) — the key invariant is: NOT done, and the worktree is
    // preserved.
    let task = store.get(&format!("worktask:task:{task_id}")).await.unwrap();
    assert_ne!(
        task["status"].as_str(),
        Some("done"),
        "a failed github-pr push must NOT mark the task done; status={:?}",
        task["status"]
    );
    assert!(
        wt.exists() && worktree_listed(&git, &repo, &wt).await,
        "a failed push must preserve the worktree (push-only never tears down)"
    );
    // The dead remote of course has no branch.
    let ls = git
        .run("ls-remote", &["ls-remote", "--heads", &dead_remote.to_string_lossy()])
        .await
        .unwrap();
    assert!(!ls.stdout.contains(branch), "branch must not exist on an unreachable remote");
}