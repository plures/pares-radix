//! Integration tests for TUI session persistence with PluresDbStateStore on disk.
//!
//! Verifies that sessions survive a full save → drop → reopen cycle,
//! confirming durable cross-restart persistence.

use std::sync::Arc;

use pares_agens_core::model::ChatMessage;
use pares_agens_core::session::{SessionManager, SessionMetadata};
use pares_agens_core::{PluresDbStateStore, StateStore};

fn temp_db_path() -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("pares-radix-session-test-{}", std::process::id()));
    path
}

fn make_metadata(count: usize, topic: &str) -> SessionMetadata {
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

/// Save a session to disk, drop the store, reopen it, and verify messages persist.
#[tokio::test]
async fn save_and_reload_from_disk() {
    let db_path = temp_db_path();
    let _ = std::fs::remove_dir_all(&db_path); // Clean slate

    let chat_id = "tui:default";
    let messages = vec![
        ChatMessage::user("Hello, how are you?"),
        ChatMessage::assistant("I'm doing well, thanks for asking!"),
        ChatMessage::user("Can you help me with Rust?"),
        ChatMessage::assistant("Of course! What do you need help with?"),
    ];

    // Phase 1: Save session
    {
        let store = PluresDbStateStore::open(&db_path).expect("open store");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);
        mgr.save_session(chat_id, &messages, make_metadata(4, "default"))
            .await;
    }
    // Store is dropped here, simulating process exit

    // Phase 2: Reopen and verify
    {
        let store = PluresDbStateStore::open(&db_path).expect("reopen store");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);
        let loaded = mgr
            .load_active_session(chat_id)
            .await
            .expect("session should persist across reopen");

        assert_eq!(loaded.messages.len(), 4);
        assert_eq!(loaded.messages[0].role, "user");
        assert_eq!(loaded.messages[0].content, "Hello, how are you?");
        assert_eq!(loaded.messages[1].role, "assistant");
        assert_eq!(
            loaded.messages[1].content,
            "I'm doing well, thanks for asking!"
        );
        assert_eq!(loaded.messages[2].content, "Can you help me with Rust?");
        assert_eq!(
            loaded.messages[3].content,
            "Of course! What do you need help with?"
        );
        assert_eq!(loaded.metadata.message_count, 4);
        assert_eq!(loaded.metadata.topic_summary, Some("default".to_string()));
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&db_path);
}

/// Multiple named sessions persist independently.
#[tokio::test]
async fn multiple_sessions_persist_independently() {
    let db_path = temp_db_path().join("multi");
    let _ = std::fs::remove_dir_all(&db_path);

    let work_messages = vec![
        ChatMessage::user("Let's discuss the API design"),
        ChatMessage::assistant("Sure, what aspects would you like to cover?"),
    ];
    let personal_messages = vec![
        ChatMessage::user("What's the weather like?"),
        ChatMessage::assistant("I'd need to check a weather service for that."),
        ChatMessage::user("Never mind, just curious"),
    ];

    // Save both sessions
    {
        let store = PluresDbStateStore::open(&db_path).expect("open store");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);
        mgr.save_session("tui:work", &work_messages, make_metadata(2, "work"))
            .await;
        mgr.save_session(
            "tui:personal",
            &personal_messages,
            make_metadata(3, "personal"),
        )
        .await;
    }

    // Reopen and verify each session independently
    {
        let store = PluresDbStateStore::open(&db_path).expect("reopen store");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);

        let work = mgr
            .load_active_session("tui:work")
            .await
            .expect("work session should exist");
        assert_eq!(work.messages.len(), 2);
        assert_eq!(work.metadata.topic_summary, Some("work".to_string()));

        let personal = mgr
            .load_active_session("tui:personal")
            .await
            .expect("personal session should exist");
        assert_eq!(personal.messages.len(), 3);
        assert_eq!(
            personal.metadata.topic_summary,
            Some("personal".to_string())
        );
    }

    let _ = std::fs::remove_dir_all(&db_path);
}

/// Archive + new session cycle persists correctly.
#[tokio::test]
async fn archive_and_new_session_persists() {
    let db_path = temp_db_path().join("archive");
    let _ = std::fs::remove_dir_all(&db_path);

    let chat_id = "tui:default";
    let old_messages = vec![
        ChatMessage::user("old conversation"),
        ChatMessage::assistant("old reply"),
    ];
    let new_messages = vec![
        ChatMessage::user("fresh start"),
        ChatMessage::assistant("hello again!"),
    ];

    // Save, archive, then save new session
    {
        let store = PluresDbStateStore::open(&db_path).expect("open store");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);

        // Save initial session
        mgr.save_session(chat_id, &old_messages, make_metadata(2, "default"))
            .await;
        // Archive it
        mgr.archive_session(chat_id).await;
        // Save new session on the same chat_id
        mgr.save_session(chat_id, &new_messages, make_metadata(2, "default"))
            .await;
    }

    // Reopen and verify: active = new, archived = old
    {
        let store = PluresDbStateStore::open(&db_path).expect("reopen store");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);

        // Active session should be the new one
        let active = mgr.load_active_session(chat_id).await.expect("active");
        assert_eq!(active.messages.len(), 2);
        assert_eq!(active.messages[0].content, "fresh start");

        // List should show both active and archived
        let sessions = mgr.list_sessions(chat_id, 10).await;
        assert_eq!(sessions.len(), 2);
        // First is active
        assert_eq!(sessions[0].key, "active");
        // Second is archived
        assert!(sessions[1].key.starts_with("session:archive:"));
    }

    let _ = std::fs::remove_dir_all(&db_path);
}

/// Overwriting an active session replaces it cleanly.
#[tokio::test]
async fn overwrite_session_replaces_content() {
    let db_path = temp_db_path().join("overwrite");
    let _ = std::fs::remove_dir_all(&db_path);

    let chat_id = "tui:work";

    // Save v1, then overwrite with v2, then verify — single store open
    // (sled locks the DB directory; multiple opens in the same process can race)
    {
        let store = PluresDbStateStore::open(&db_path).expect("open");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);

        // Save v1
        let msgs_v1 = vec![ChatMessage::user("version 1")];
        mgr.save_session(chat_id, &msgs_v1, make_metadata(1, "work"))
            .await;

        // Verify v1 is there
        let loaded_v1 = mgr.load_active_session(chat_id).await.expect("v1 exists");
        assert_eq!(loaded_v1.messages.len(), 1);
        assert_eq!(loaded_v1.messages[0].content, "version 1");

        // Save v2 (overwrite)
        let msgs_v2 = vec![
            ChatMessage::user("version 2 - first"),
            ChatMessage::assistant("version 2 - reply"),
            ChatMessage::user("version 2 - followup"),
        ];
        mgr.save_session(chat_id, &msgs_v2, make_metadata(3, "work"))
            .await;

        // Verify only v2 exists
        let loaded_v2 = mgr.load_active_session(chat_id).await.expect("v2 exists");
        assert_eq!(loaded_v2.messages.len(), 3);
        assert_eq!(loaded_v2.messages[0].content, "version 2 - first");
        assert_eq!(loaded_v2.metadata.message_count, 3);
    }

    // Reopen from disk and confirm v2 persisted
    {
        let store = PluresDbStateStore::open(&db_path).expect("reopen");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);
        let loaded = mgr.load_active_session(chat_id).await.expect("persisted");
        assert_eq!(loaded.messages.len(), 3);
        assert_eq!(loaded.messages[0].content, "version 2 - first");
    }

    let _ = std::fs::remove_dir_all(&db_path);
}

/// Empty session (no messages) still persists metadata correctly.
#[tokio::test]
async fn empty_session_persists_metadata() {
    let db_path = temp_db_path().join("empty");
    let _ = std::fs::remove_dir_all(&db_path);

    let chat_id = "tui:empty";

    {
        let store = PluresDbStateStore::open(&db_path).expect("open");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);
        mgr.save_session(chat_id, &[], make_metadata(0, "empty session"))
            .await;
    }

    {
        let store = PluresDbStateStore::open(&db_path).expect("open");
        let mgr = SessionManager::new(Arc::new(store) as Arc<dyn StateStore>);
        let loaded = mgr.load_active_session(chat_id).await.expect("exists");
        assert_eq!(loaded.messages.len(), 0);
        assert_eq!(loaded.metadata.message_count, 0);
        assert_eq!(
            loaded.metadata.topic_summary,
            Some("empty session".to_string())
        );
    }

    let _ = std::fs::remove_dir_all(&db_path);
}
