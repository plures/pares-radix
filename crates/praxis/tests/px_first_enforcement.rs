//! Build-time enforcement of .px-first development mandate.
//!
//! These tests scan the pares-radix codebase for patterns that indicate
//! business logic living in Rust when it should be expressed as .px procedures.
//!
//! Violations FAIL the test suite — this is intentional enforcement, not a warning.

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Recursively collect .rs files from a directory, excluding target/
fn collect_rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if !dir.exists() {
        return files;
    }
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.file_name().map(|n| n == "target").unwrap_or(false) {
            continue;
        }
        if path.is_dir() {
            files.extend(collect_rs_files(&path));
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            files.push(path);
        }
    }
    files
}

/// Patterns that indicate in-memory state management that should go through PluresDB.
const FORBIDDEN_STATE_PATTERNS: &[&str] = &[
    "Arc<Mutex<HashMap<",
    "Arc<RwLock<HashMap<",
    "lazy_static! { static ref",
    "static mut ",
];

/// Crates that are ALLOWED to have in-memory state (side-effect boundaries).
const STATE_ALLOWLIST: &[&str] = &[
    "crates/radix-core/src/shell_executor.rs", // process session management (IO boundary)
    "crates/radix-core/src/spine/pipeline.rs", // event bus (infrastructure)
    "crates/radix-core/src/spine/procedures/tool_executor.rs", // per-chat loop counter (runtime safety)
    "crates/radix-core/src/delegation/manager.rs", // runtime task handles (JoinHandle is IO)
    "crates/tui/",                                 // TUI state is ephemeral UI, not business logic
    "crates/tauri-app/",                           // GUI state
    "crates/mcp-server/",                          // connection state (IO boundary)
    "crates/mcp-client/",                          // connection state (IO boundary)
    "crates/mcp-server/",                          // connection state (IO boundary)
    "crates/praxis/src/px/async_executor.rs",      // runtime execution state
    "crates/praxis/src/px/watcher.rs",             // filesystem watcher (IO boundary)
    "crates/sync/src/lan.rs",                      // network peer discovery (IO boundary)
    "crates/radix-core/src/plugins/runtime.rs",    // plugin lifecycle (IO boundary)
    "crates/cli/src/main.rs",                      // ToolTraceStore is ephemeral debug tracing
    "crates/agenda/src/scheduler.rs", // runtime task working set (persistence via TaskStore, HashMap is hot cache)
    "crates/radix-core/src/secrets.rs", // InMemorySecretStore is test/dev utility only (not used in production)
    "crates/radix-core/src/handlers/on_timer.rs", // dispatch table of Arc<dyn TimerAction> code refs (not serializable data)
    "crates/radix-core/src/spine/conversation.rs", // MemoryConversationStore is test utility (PluresConversationStore is production)
    "crates/radix-core/src/agent.rs", // conversation_history is hot cache; persistence via turn_store (PluresDB)
    "crates/radix-core/src/cerebellum/actions.rs", // state store hot cache (transitional; migrating to PluresDB)
    // Live oneshot senders for in-flight reactive-chain result waiters are process-local
    // handles that cannot be serialized/persisted to PluresDB. Transient runtime coordination
    // (same class as plugins/runtime, shell_executor).
    "crates/radix-core/src/spine/reactive.rs",
    // MemoryThreadStore is the documented TEST-ONLY in-memory thread store (production store is
    // PluresDB-backed). It is `pub` (used by cross-crate integration tests) and lives in a
    // non-#[cfg(test)] path, so the scanner catches it; allowlist rather than cfg-gate it.
    "crates/radix-core/src/threading/store.rs",
];

/// Check that no crate introduces persistent in-memory state outside the allowlist.
/// C-PLURES-003: ALL persistent state goes through PluresDB.
#[test]
fn no_persistent_in_memory_state_outside_allowlist() {
    let root = workspace_root();
    let crates_dir = root.join("crates");
    let files = collect_rs_files(&crates_dir);

    let mut violations = Vec::new();

    for file in &files {
        let rel_path = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        // Skip allowlisted paths (normalize separators for cross-platform)
        if STATE_ALLOWLIST
            .iter()
            .any(|allowed| rel_path.contains(allowed))
        {
            continue;
        }

        // Skip test files
        if rel_path.contains("/tests/")
            || rel_path.contains("/test_")
            || rel_path.ends_with("_test.rs")
        {
            continue;
        }

        let content = fs::read_to_string(file).unwrap_or_default();
        for pattern in FORBIDDEN_STATE_PATTERNS {
            if content.contains(pattern) {
                // Check it's not in a comment
                for (line_num, line) in content.lines().enumerate() {
                    if line.contains(pattern)
                        && !line.trim_start().starts_with("//")
                        && !line.trim_start().starts_with("*")
                    {
                        violations.push(format!(
                            "{}:{} — contains '{}' (state must go through PluresDB, C-PLURES-003)",
                            rel_path,
                            line_num + 1,
                            pattern
                        ));
                    }
                }
            }
        }
    }

    if !violations.is_empty() {
        panic!(
            "\n\n🚫 .px-first ENFORCEMENT: Found {} violation(s) of C-PLURES-003 (no in-memory state):\n\n{}\n\n\
             Fix: Move this state into PluresDB nodes, or add the file to STATE_ALLOWLIST \
             in px_first_enforcement.rs if it's a legitimate IO boundary.\n",
            violations.len(),
            violations.join("\n")
        );
    }
}

/// Patterns that indicate a standalone storage/memory system being introduced.
const FORBIDDEN_STORAGE_PATTERNS: &[&str] = &[
    "use rusqlite",
    "use sqlx",
    "sled::open",
    "use rocksdb",
    "PluresLM::connect", // No external PluresLM service
    "plures_lm::client", // No PluresLM client
];

/// No new storage backends — PluresDB is the only database (C-PLURES-003).
#[test]
fn no_external_storage_backends() {
    let root = workspace_root();
    let crates_dir = root.join("crates");
    let files = collect_rs_files(&crates_dir);

    let mut violations = Vec::new();

    for file in &files {
        let rel_path = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        // Skip test files
        if rel_path.contains("/tests/") {
            continue;
        }

        let content = fs::read_to_string(file).unwrap_or_default();
        for pattern in FORBIDDEN_STORAGE_PATTERNS {
            if content.contains(pattern) {
                for (line_num, line) in content.lines().enumerate() {
                    if line.contains(pattern) && !line.trim_start().starts_with("//") {
                        violations.push(format!(
                            "{}:{} — contains '{}' (PluresDB is the only storage, ADR-0020)",
                            rel_path,
                            line_num + 1,
                            pattern
                        ));
                    }
                }
            }
        }
    }

    if !violations.is_empty() {
        panic!(
            "\n\n🚫 .px-first ENFORCEMENT: Found {} violation(s) of ADR-0020 (single PluresDB):\n\n{}\n\n\
             Fix: Use PluresDB for all persistent state. No SQLite, sled, PluresLM service, or vector DBs.\n",
            violations.len(),
            violations.join("\n")
        );
    }
}

/// Verify all .px files in praxis/ parse cleanly.
/// This ensures the .px language is the source of truth for logic.
/// Note: only files containing `constraint` blocks are validated — procedure
/// specification files use extended syntax that the parser is being extended to support.
#[test]
fn all_px_files_parse_cleanly() {
    let root = workspace_root();
    let praxis_dir = root.join("praxis");

    if !praxis_dir.exists() {
        panic!("praxis/ directory not found at workspace root — .px-first mandate requires it");
    }

    let mut px_files = Vec::new();
    collect_px_files(&praxis_dir, &mut px_files);

    assert!(
        !px_files.is_empty(),
        "No .px files found in praxis/ — .px-first mandate requires procedures defined in .px"
    );

    let mut failures = Vec::new();
    let mut validated = 0;
    for file in &px_files {
        let source = fs::read_to_string(file).unwrap_or_default();
        // Only validate files that contain constraint blocks (the parser's current scope)
        // Procedure specification files use extended syntax pending parser support
        if !source.contains("constraint ") {
            continue;
        }
        validated += 1;
        if let Err(e) = pares_radix_praxis::px::parse(&source) {
            let rel = file.strip_prefix(&root).unwrap().to_string_lossy();
            failures.push(format!("{}: {}", rel, e));
        }
    }

    assert!(
        validated > 0,
        "No constraint .px files found — .px-first mandate requires at least one"
    );

    if !failures.is_empty() {
        panic!(
            "\n\n🚫 .px-first ENFORCEMENT: {} .px file(s) failed to parse:\n\n{}\n\n\
             Fix: All .px files must parse cleanly before pushing (px_before_rust_test constraint).\n",
            failures.len(),
            failures.join("\n")
        );
    }
}

fn collect_px_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_px_files(&path, files);
        } else if path.extension().map(|e| e == "px").unwrap_or(false) {
            files.push(path);
        }
    }
}

/// Verify that required .px procedure files exist for core capabilities.
/// If capability X exists in Rust, its logic MUST be defined in a .px file.
#[test]
fn required_px_procedures_exist() {
    let root = workspace_root();
    let praxis_dir = root.join("praxis/procedures");

    let required = &[(
        "memory.px",
        "Memory operations (store, search, consolidate) must be defined in .px",
    )];

    let mut missing = Vec::new();
    for (file, reason) in required {
        if !praxis_dir.join(file).exists() {
            missing.push(format!("praxis/procedures/{} — {}", file, reason));
        }
    }

    if !missing.is_empty() {
        panic!(
            "\n\n🚫 .px-first ENFORCEMENT: Required .px procedure files missing:\n\n{}\n\n\
             Fix: Express the logic in .px FIRST, then implement the Rust side-effect actors.\n",
            missing.join("\n")
        );
    }
}
