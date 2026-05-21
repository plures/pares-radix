//! Integration tests for TUI keyboard shortcuts:
//! - Ctrl+L equivalent: `clear_chat()`
//! - Ctrl+U equivalent: `clear_input()`
//! - Alt+N equivalent: `switch_to_index(N-1)`
//! - Alt+Enter equivalent: `insert_newline()`

use std::sync::Arc;

use async_trait::async_trait;
use pares_agens_core::agent::{Agent, Memory};
use pares_agens_core::model::{
    ChatMessage as CoreChatMessage, ChatOptions, ModelClient, ModelCompletion, ToolDefinition,
    ToolDispatcher,
};
use pares_radix_tui::app::{App, AppEvent, ChatMessage, InputHistory, Role};
use serde_json::Value;
use tokio::sync::mpsc;

// ─── Mock infrastructure ──────────────────────────────────────────────────────

struct NoopMemory;
#[async_trait]
impl Memory for NoopMemory {
    async fn capture(&self, _content: &str) -> Result<(), String> {
        Ok(())
    }
    async fn recall(&self, _query: &str) -> Result<Vec<String>, String> {
        Ok(vec![])
    }
}

struct NoopModel;
#[async_trait]
impl ModelClient for NoopModel {
    async fn complete(
        &self,
        _messages: &[CoreChatMessage],
        _tools: &[ToolDefinition],
        _options: &ChatOptions,
    ) -> Result<ModelCompletion, String> {
        Ok(ModelCompletion {
            content: Some("hi".into()),
            tool_calls: vec![],
            logprobs: None,
            model: None,
        })
    }
}

struct NoopDispatcher;
#[async_trait]
impl ToolDispatcher for NoopDispatcher {
    async fn available_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }
    async fn call_tool(&self, _name: &str, _args: Value) -> String {
        "ok".into()
    }
}

fn make_app() -> (App, mpsc::UnboundedReceiver<AppEvent>) {
    let agent = Arc::new(
        Agent::new(Arc::new(NoopMemory))
            .with_model(Arc::new(NoopModel), Arc::new(NoopDispatcher), "test".to_string()),
    );
    let (tx, rx) = mpsc::unbounded_channel();
    let app = App {
        messages: vec![ChatMessage {
            role: Role::System,
            content: "Test TUI ready.".to_string(),
            timestamp: chrono::Utc::now(),
        }],
        input: String::new(),
        input_cursor: 0,
        scroll_offset: 0,
        user_scrolled: false,
        viewport_height: 35,
        thinking: false,
        streaming: false,
        current_model: "test-model".to_string(),
        agent,
        event_tx: tx,
        history: InputHistory::new(500),
        sessions: vec![("default".to_string(), true)],
        current_session: "default".to_string(),
        session_manager: None,
    };
    (app, rx)
}

// ─── Ctrl+L: clear_chat ───────────────────────────────────────────────────────

#[test]
fn clear_chat_resets_messages_and_scroll() {
    let (mut app, _rx) = make_app();
    app.push_system("Hello");
    app.push_system("World");
    app.scroll_offset = 3;
    app.user_scrolled = true;

    assert!(app.messages.len() >= 3);

    app.clear_chat();

    // Only the "Chat cleared." system message remains
    assert_eq!(app.messages.len(), 1);
    assert!(app.messages[0].content.contains("Chat cleared."));
    assert_eq!(app.scroll_offset, 0);
    assert!(!app.user_scrolled);
}

#[test]
fn clear_chat_on_empty_still_adds_message() {
    let (mut app, _rx) = make_app();
    app.messages.clear();

    app.clear_chat();

    assert_eq!(app.messages.len(), 1);
    assert!(app.messages[0].content.contains("Chat cleared."));
}

// ─── Ctrl+U: clear_input ─────────────────────────────────────────────────────

#[test]
fn clear_input_empties_buffer() {
    let (mut app, _rx) = make_app();
    app.input = "some partial input".to_string();
    app.input_cursor = 10;

    app.clear_input();

    assert!(app.input.is_empty());
    assert_eq!(app.input_cursor, 0);
}

#[test]
fn clear_input_on_empty_is_safe() {
    let (mut app, _rx) = make_app();
    app.clear_input();
    assert!(app.input.is_empty());
    assert_eq!(app.input_cursor, 0);
}

// ─── Alt+Enter: insert_newline ────────────────────────────────────────────────

#[test]
fn insert_newline_at_cursor_midpoint() {
    let (mut app, _rx) = make_app();
    app.input = "hello world".to_string();
    app.input_cursor = 5;

    app.insert_newline();

    assert_eq!(app.input, "hello\n world");
    assert_eq!(app.input_cursor, 6);
}

#[test]
fn insert_newline_at_end() {
    let (mut app, _rx) = make_app();
    app.input = "line1".to_string();
    app.input_cursor = 5;

    app.insert_newline();

    assert_eq!(app.input, "line1\n");
    assert_eq!(app.input_cursor, 6);
}

// ─── Alt+N: switch_to_index ──────────────────────────────────────────────────

#[test]
fn switch_to_index_changes_active_session() {
    let (mut app, _rx) = make_app();
    app.sessions = vec![
        ("default".to_string(), true),
        ("work".to_string(), false),
        ("research".to_string(), false),
    ];
    app.current_session = "default".to_string();

    app.switch_to_index(1); // → work

    assert_eq!(app.current_session, "work");
    assert!(!app.sessions[0].1);
    assert!(app.sessions[1].1);
    assert!(!app.sessions[2].1);
    assert!(app.messages.iter().any(|m| m.content.contains("Switched to session: work")));
}

#[test]
fn switch_to_index_out_of_bounds_is_noop() {
    let (mut app, _rx) = make_app();
    app.sessions = vec![("default".to_string(), true)];
    app.current_session = "default".to_string();

    app.switch_to_index(4);

    assert_eq!(app.current_session, "default");
    assert!(app.sessions[0].1);
}

#[test]
fn switch_to_index_same_session_is_noop() {
    let (mut app, _rx) = make_app();
    app.sessions = vec![
        ("default".to_string(), true),
        ("work".to_string(), false),
    ];
    app.current_session = "default".to_string();
    let msg_count = app.messages.len();

    app.switch_to_index(0); // already on default

    assert_eq!(app.current_session, "default");
    assert_eq!(app.messages.len(), msg_count);
}

#[test]
fn switch_to_index_preserves_order() {
    let (mut app, _rx) = make_app();
    app.sessions = vec![
        ("alpha".to_string(), true),
        ("beta".to_string(), false),
        ("gamma".to_string(), false),
    ];
    app.current_session = "alpha".to_string();

    app.switch_to_index(2); // → gamma

    assert_eq!(app.current_session, "gamma");
    assert_eq!(app.sessions[0].0, "alpha");
    assert_eq!(app.sessions[1].0, "beta");
    assert_eq!(app.sessions[2].0, "gamma");
    assert!(!app.sessions[0].1);
    assert!(!app.sessions[1].1);
    assert!(app.sessions[2].1);
}

#[test]
fn sequential_switches_work() {
    let (mut app, _rx) = make_app();
    app.sessions = vec![
        ("s1".to_string(), true),
        ("s2".to_string(), false),
        ("s3".to_string(), false),
    ];
    app.current_session = "s1".to_string();

    app.switch_to_index(1);
    assert_eq!(app.current_session, "s2");

    app.switch_to_index(2);
    assert_eq!(app.current_session, "s3");

    app.switch_to_index(0);
    assert_eq!(app.current_session, "s1");
}

// ─── Ctrl+W: delete_word_backward ─────────────────────────────────────────────

#[test]
fn delete_word_backward_removes_last_word() {
    let (mut app, _rx) = make_app();
    app.input = "hello world".to_string();
    app.input_cursor = app.input.len(); // cursor at end

    app.delete_word_backward();

    assert_eq!(app.input, "hello ");
    assert_eq!(app.input_cursor, 6);
}

#[test]
fn delete_word_backward_removes_single_word() {
    let (mut app, _rx) = make_app();
    app.input = "hello".to_string();
    app.input_cursor = 5;

    app.delete_word_backward();

    assert_eq!(app.input, "");
    assert_eq!(app.input_cursor, 0);
}

#[test]
fn delete_word_backward_at_start_is_noop() {
    let (mut app, _rx) = make_app();
    app.input = "hello world".to_string();
    app.input_cursor = 0;

    app.delete_word_backward();

    assert_eq!(app.input, "hello world");
    assert_eq!(app.input_cursor, 0);
}

#[test]
fn delete_word_backward_on_empty_is_noop() {
    let (mut app, _rx) = make_app();
    app.input = String::new();
    app.input_cursor = 0;

    app.delete_word_backward();

    assert_eq!(app.input, "");
    assert_eq!(app.input_cursor, 0);
}

#[test]
fn delete_word_backward_handles_trailing_spaces() {
    let (mut app, _rx) = make_app();
    app.input = "one two   ".to_string();
    app.input_cursor = app.input.len(); // cursor after trailing spaces

    app.delete_word_backward();

    // Trims trailing spaces, then deletes back to previous word boundary
    assert_eq!(app.input, "one ");
    assert_eq!(app.input_cursor, 4);
}

#[test]
fn delete_word_backward_cursor_midword() {
    let (mut app, _rx) = make_app();
    app.input = "hello world".to_string();
    app.input_cursor = 8; // cursor after "wor"

    app.delete_word_backward();

    // "hello wo|rld" → trim_end of "hello wo" = "hello wo",
    // rfind whitespace = 5 → new_end = 6
    assert_eq!(app.input, "hello rld");
    assert_eq!(app.input_cursor, 6);
}

#[test]
fn delete_word_backward_multiple_times() {
    let (mut app, _rx) = make_app();
    app.input = "one two three".to_string();
    app.input_cursor = app.input.len();

    app.delete_word_backward();
    assert_eq!(app.input, "one two ");
    assert_eq!(app.input_cursor, 8);

    app.delete_word_backward();
    assert_eq!(app.input, "one ");
    assert_eq!(app.input_cursor, 4);

    app.delete_word_backward();
    assert_eq!(app.input, "");
    assert_eq!(app.input_cursor, 0);
}
