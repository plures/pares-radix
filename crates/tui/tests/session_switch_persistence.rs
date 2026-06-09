//! Integration tests for TUI session switch + load messages from disk roundtrip.
//!
//! Verifies multi-session navigation with persistence: switching sessions
//! triggers save of the current session and load of the target session's
//! messages from the PluresDbStateStore.

use std::sync::Arc;

use pares_agens_core::model::ChatMessage;
use pares_agens_core::session::{SessionManager, SessionMetadata};
use pares_agens_core::{PluresDbStateStore, StateStore};

fn temp_db_path(suffix: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "pares-radix-tui-switch-test-{}-{}",
        std::process::id(),
        suffix
    ));
    path
}

fn metadata(count: usize, topic: &str) -> SessionMetadata {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    SessionMetadata {
        started_at: now,
        last_message_at: now,
        message_count: count,
        topic_summary: Some(topic.to_string()),
    }
}

/// Full roundtrip: save two sessions, "switch" by loading the other, verify messages match.
#[tokio::test]
async fn switch_session_loads_correct_messages() {
    let db_path = temp_db_path("switch-correct");
    let _ = std::fs::remove_dir_all(&db_path);

    let store = PluresDbStateStore::open(&db_path).expect("open store");
    let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);

    // Create session "work"
    let work_msgs = vec![
        ChatMessage::user("Let's discuss the API"),
        ChatMessage::assistant("Sure! What aspect?"),
        ChatMessage::user("Error handling patterns"),
    ];
    mgr.save_session("tui:work", &work_msgs, metadata(3, "work"))
        .await;

    // Create session "personal"
    let personal_msgs = vec![
        ChatMessage::user("What's for dinner?"),
        ChatMessage::assistant("How about pasta?"),
    ];
    mgr.save_session("tui:personal", &personal_msgs, metadata(2, "personal"))
        .await;

    // Simulate switching from "work" → "personal": load personal's messages
    let loaded = mgr
        .load_active_session("tui:personal")
        .await
        .expect("personal session should exist");
    assert_eq!(loaded.messages.len(), 2);
    assert_eq!(loaded.messages[0].content, "What's for dinner?");
    assert_eq!(loaded.messages[1].content, "How about pasta?");

    // Simulate switching from "personal" → "work": load work's messages
    let loaded = mgr
        .load_active_session("tui:work")
        .await
        .expect("work session should exist");
    assert_eq!(loaded.messages.len(), 3);
    assert_eq!(loaded.messages[0].content, "Let's discuss the API");
    assert_eq!(loaded.messages[2].content, "Error handling patterns");

    let _ = std::fs::remove_dir_all(&db_path);
}

/// Switch-save-switch: modifying a session and switching away persists the changes.
#[tokio::test]
async fn switch_away_persists_then_switch_back_sees_updates() {
    let db_path = temp_db_path("switch-persist");
    let _ = std::fs::remove_dir_all(&db_path);

    let store = PluresDbStateStore::open(&db_path).expect("open store");
    let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);

    // Save initial "work" session with 2 messages
    let initial_msgs = vec![ChatMessage::user("hello"), ChatMessage::assistant("hi!")];
    mgr.save_session("tui:work", &initial_msgs, metadata(2, "work"))
        .await;

    // Save "personal" session
    mgr.save_session(
        "tui:personal",
        &[ChatMessage::user("personal msg")],
        metadata(1, "personal"),
    )
    .await;

    // User is on "work", adds messages, then switches to "personal"
    // (TUI calls persist_current_session before switching)
    let updated_work_msgs = vec![
        ChatMessage::user("hello"),
        ChatMessage::assistant("hi!"),
        ChatMessage::user("new question"),
        ChatMessage::assistant("new answer"),
    ];
    mgr.save_session("tui:work", &updated_work_msgs, metadata(4, "work"))
        .await;

    // Now switch to "personal"
    let personal = mgr
        .load_active_session("tui:personal")
        .await
        .expect("personal exists");
    assert_eq!(personal.messages.len(), 1);

    // Switch back to "work" — should see the updated 4 messages
    let work = mgr
        .load_active_session("tui:work")
        .await
        .expect("work exists");
    assert_eq!(work.messages.len(), 4);
    assert_eq!(work.messages[2].content, "new question");
    assert_eq!(work.messages[3].content, "new answer");

    let _ = std::fs::remove_dir_all(&db_path);
}

/// Switching to a session that was archived returns None (active session absent).
#[tokio::test]
async fn switch_to_archived_session_returns_none() {
    let db_path = temp_db_path("switch-archived");
    let _ = std::fs::remove_dir_all(&db_path);

    let store = PluresDbStateStore::open(&db_path).expect("open store");
    let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);

    // Save and archive
    mgr.save_session(
        "tui:old",
        &[ChatMessage::user("old stuff")],
        metadata(1, "old"),
    )
    .await;
    mgr.archive_session("tui:old").await;

    // Attempting to load active session should return None (it's archived)
    let result = mgr.load_active_session("tui:old").await;
    assert!(
        result.is_none(),
        "archived session should not be loadable as active"
    );

    let _ = std::fs::remove_dir_all(&db_path);
}

/// Multi-session navigation across process restart (close → reopen → sessions intact).
#[tokio::test]
async fn sessions_survive_process_restart() {
    let db_path = temp_db_path("switch-restart");
    let _ = std::fs::remove_dir_all(&db_path);

    // Phase 1: Create multiple sessions and "exit"
    {
        let store = PluresDbStateStore::open(&db_path).expect("open store");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);

        mgr.save_session(
            "tui:project-a",
            &[
                ChatMessage::user("project A discussion"),
                ChatMessage::assistant("Let's plan project A"),
            ],
            metadata(2, "project-a"),
        )
        .await;

        mgr.save_session(
            "tui:project-b",
            &[
                ChatMessage::user("project B notes"),
                ChatMessage::assistant("Here are B's details"),
                ChatMessage::user("more details please"),
            ],
            metadata(3, "project-b"),
        )
        .await;

        mgr.save_session(
            "tui:scratch",
            &[ChatMessage::user("quick test")],
            metadata(1, "scratch"),
        )
        .await;
    }
    // Store dropped — simulates process exit

    // Phase 2: Reopen and navigate between sessions
    {
        let store = PluresDbStateStore::open(&db_path).expect("reopen store");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);

        // "Switch" to project-a
        let a = mgr
            .load_active_session("tui:project-a")
            .await
            .expect("project-a should persist");
        assert_eq!(a.messages.len(), 2);
        assert_eq!(a.messages[0].content, "project A discussion");

        // "Switch" to project-b
        let b = mgr
            .load_active_session("tui:project-b")
            .await
            .expect("project-b should persist");
        assert_eq!(b.messages.len(), 3);
        assert_eq!(b.messages[2].content, "more details please");

        // "Switch" to scratch
        let s = mgr
            .load_active_session("tui:scratch")
            .await
            .expect("scratch should persist");
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages[0].content, "quick test");
    }

    let _ = std::fs::remove_dir_all(&db_path);
}

/// Rapid session switching doesn't corrupt data (simulates quick Alt+1, Alt+2 etc).
#[tokio::test]
async fn rapid_session_switching_preserves_integrity() {
    let db_path = temp_db_path("switch-rapid");
    let _ = std::fs::remove_dir_all(&db_path);

    let store = PluresDbStateStore::open(&db_path).expect("open store");
    let mgr = Arc::new(SessionManager::new(Arc::new(store) as Arc<dyn StateStore>));

    // Pre-populate 5 sessions
    for i in 0..5 {
        let msgs: Vec<ChatMessage> = (0..=i)
            .map(|j| {
                let s = format!("session-{i} message-{j}");
                ChatMessage::user(&s)
            })
            .collect();
        mgr.save_session(
            &format!("tui:s{i}"),
            &msgs,
            metadata(msgs.len(), &format!("s{i}")),
        )
        .await;
    }

    // Rapidly switch between sessions (simulating quick navigation)
    for round in 0..3 {
        for i in 0..5 {
            let loaded = mgr
                .load_active_session(&format!("tui:s{i}"))
                .await
                .unwrap_or_else(|| panic!("s{i} should exist on round {round}"));
            // Each session should have i+1 messages
            assert_eq!(loaded.messages.len(), i + 1);
            assert_eq!(loaded.messages[0].content, format!("session-{i} message-0"));
        }
    }

    let _ = std::fs::remove_dir_all(&db_path);
}
