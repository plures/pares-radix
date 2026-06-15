//! Thread command parser — handles /thread and /threads user commands.
//!
//! Commands:
//!   /threads                    — list active threads
//!   /thread new [topic]         — create a new thread (optional topic name)
//!   /thread switch <id|name>    — switch to a thread by ID or topic name
//!   /thread close [id|name]     — archive a thread
//!   /thread context             — show current thread info
//!   /thread rename <new_name>   — rename the active thread
//!   /thread search <query>      — search across all thread histories
//!   /thread merge <id1> <id2>   — merge two threads (future)

use std::sync::Arc;

use crate::model::ChatMessage;
use crate::threading::store::ThreadStore;
use crate::threading::types::{Thread, ThreadState};

/// Parsed thread command variants.
#[derive(Debug, Clone, PartialEq)]
pub enum ThreadCommand {
    List,
    New { topic: Option<String> },
    Switch { target: String },
    Close { target: Option<String> },
    Context,
    Rename { new_name: String },
    Search { query: String },
    Help,
    Invalid { input: String },
}

impl ThreadCommand {
    /// Parse a message into a ThreadCommand, or None if it's not a thread command.
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();

        if trimmed == "/threads" {
            return Some(Self::List);
        }

        // Must start with "/thread" followed by end-of-string or whitespace.
        // "/threadnew" (no space) should NOT match.
        if !trimmed.starts_with("/thread") {
            return None;
        }

        let after_thread = &trimmed["/thread".len()..];
        // Must be exactly "/thread" or "/thread " followed by subcommand
        if !after_thread.is_empty() && !after_thread.starts_with(' ') {
            // Could be "/threads" (already handled) or "/threadfoo" (invalid prefix)
            return None;
        }

        let rest = after_thread.trim();

        if rest.is_empty() {
            return Some(Self::Help);
        }

        // Parse subcommands
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        let subcommand = parts[0];
        let arg = parts.get(1).map(|s| s.trim().to_string());

        match subcommand {
            "new" | "n" => Some(Self::New { topic: arg }),
            "switch" | "s" | "sw" => Some(Self::Switch {
                target: arg.unwrap_or_default(),
            }),
            "close" | "c" | "archive" => Some(Self::Close { target: arg }),
            "context" | "ctx" | "?" => Some(Self::Context),
            "rename" | "r" => Some(Self::Rename {
                new_name: arg.unwrap_or_default(),
            }),
            "search" | "find" | "q" => Some(Self::Search {
                query: arg.unwrap_or_default(),
            }),
            "help" | "h" => Some(Self::Help),
            _ => Some(Self::Invalid {
                input: rest.to_string(),
            }),
        }
    }
}

/// Execute a thread command and return a formatted response.
pub struct ThreadCommandExecutor {
    store: Arc<dyn ThreadStore>,
}

impl ThreadCommandExecutor {
    pub fn new(store: Arc<dyn ThreadStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, chat_id: &str, command: &ThreadCommand) -> ThreadCommandResult {
        match command {
            ThreadCommand::List => {
                let threads = self.store.list_threads(chat_id).await;
                let active = self.store.active_thread(chat_id).await;
                let active_id = active.map(|t| t.id).unwrap_or_default();
                ThreadCommandResult::ThreadList {
                    threads,
                    active_id,
                }
            }
            ThreadCommand::New { topic } => {
                let topic_str = topic.as_deref().unwrap_or("untitled");
                let thread = self.store.create_thread(chat_id, topic_str).await;
                ThreadCommandResult::Created { thread }
            }
            ThreadCommand::Switch { target } => {
                if target.is_empty() {
                    return ThreadCommandResult::Error {
                        message: "Usage: /thread switch <name|id>".to_string(),
                    };
                }

                // Try to find by ID first, then by topic name
                let threads = self.store.list_threads(chat_id).await;
                let found = threads.iter().find(|t| {
                    t.id == *target
                        || t.topic.to_lowercase().contains(&target.to_lowercase())
                        || t.id.starts_with(target.as_str())
                });

                match found {
                    Some(thread) => {
                        let _ = self.store.switch_thread(chat_id, &thread.id).await;
                        ThreadCommandResult::Switched {
                            thread: thread.clone(),
                        }
                    }
                    None => ThreadCommandResult::Error {
                        message: format!("No thread matching '{target}'"),
                    },
                }
            }
            ThreadCommand::Close { target } => {
                let thread_id = if let Some(t) = target {
                    let threads = self.store.list_threads(chat_id).await;
                    threads
                        .iter()
                        .find(|th| {
                            th.id == *t
                                || th.topic.to_lowercase().contains(&t.to_lowercase())
                                || th.id.starts_with(t.as_str())
                        })
                        .map(|th| th.id.clone())
                } else {
                    self.store.active_thread(chat_id).await.map(|t| t.id)
                };

                match thread_id {
                    Some(id) => {
                        let _ = self.store.archive_thread(chat_id, &id).await;
                        ThreadCommandResult::Closed { thread_id: id }
                    }
                    None => ThreadCommandResult::Error {
                        message: format!(
                            "No thread matching '{}'",
                            target.as_deref().unwrap_or("")
                        ),
                    },
                }
            }
            ThreadCommand::Context => {
                let active = self.store.active_thread(chat_id).await;
                match active {
                    Some(thread) => {
                        let history =
                            self.store.thread_history(chat_id, &thread.id).await;
                        ThreadCommandResult::Context {
                            thread,
                            message_count: history.len(),
                        }
                    }
                    None => ThreadCommandResult::Error {
                        message: "No active thread.".to_string(),
                    },
                }
            }
            ThreadCommand::Rename { new_name } => {
                if new_name.is_empty() {
                    return ThreadCommandResult::Error {
                        message: "Usage: /thread rename <new_name>".to_string(),
                    };
                }
                // ThreadStore doesn't have rename yet — return not-implemented
                ThreadCommandResult::Error {
                    message: "Rename not yet implemented. Current thread topic stays as-is."
                        .to_string(),
                }
            }
            ThreadCommand::Search { query } => {
                if query.is_empty() {
                    return ThreadCommandResult::Error {
                        message: "Usage: /thread search <query>".to_string(),
                    };
                }
                // Search across all thread histories for the query
                let threads = self.store.list_threads(chat_id).await;
                let mut results = Vec::new();

                for thread in &threads {
                    let history = self.store.thread_history(chat_id, &thread.id).await;
                    let match_count = history
                        .iter()
                        .filter(|msg| {
                            msg.content.to_lowercase().contains(&query.to_lowercase())
                        })
                        .count();
                    if match_count > 0 {
                        results.push(SearchResult {
                            thread_id: thread.id.clone(),
                            thread_topic: thread.topic.clone(),
                            match_count,
                        });
                    }
                }

                ThreadCommandResult::SearchResults {
                    query: query.clone(),
                    results,
                }
            }
            ThreadCommand::Help => ThreadCommandResult::Help,
            ThreadCommand::Invalid { input } => ThreadCommandResult::Error {
                message: format!("Unknown thread command: '{input}'. Try /thread help"),
            },
        }
    }
}

/// Result of executing a thread command.
#[derive(Debug, Clone)]
pub enum ThreadCommandResult {
    ThreadList {
        threads: Vec<Thread>,
        active_id: String,
    },
    Created {
        thread: Thread,
    },
    Switched {
        thread: Thread,
    },
    Closed {
        thread_id: String,
    },
    Context {
        thread: Thread,
        message_count: usize,
    },
    SearchResults {
        query: String,
        results: Vec<SearchResult>,
    },
    Help,
    Error {
        message: String,
    },
}

/// A search hit across threads.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub thread_id: String,
    pub thread_topic: String,
    pub match_count: usize,
}

impl ThreadCommandResult {
    /// Format the result for text-based channels (stdio, basic telegram).
    pub fn format_text(&self) -> String {
        match self {
            Self::ThreadList { threads, active_id } => {
                if threads.is_empty() {
                    return "No active threads.".to_string();
                }
                let mut out = String::from("📋 Active Threads:\n");
                for (i, t) in threads.iter().enumerate() {
                    let active_marker =
                        if t.id == *active_id { " ← active" } else { "" };
                    let state_icon = match t.state {
                        ThreadState::Active => "",
                        ThreadState::Paused => " ⏸",
                        ThreadState::Archived => " 📦",
                    };
                    out.push_str(&format!(
                        "{}. {} ({} msgs){}{}\n",
                        i + 1,
                        t.topic,
                        t.message_count,
                        active_marker,
                        state_icon
                    ));
                }
                out.push_str("\nUse /thread switch <number|name> to change.");
                out
            }
            Self::Created { thread } => {
                format!("✨ Created thread: \"{}\" (now active)", thread.topic)
            }
            Self::Switched { thread } => {
                format!(
                    "🔀 Switched to: \"{}\" ({} msgs)",
                    thread.topic, thread.message_count
                )
            }
            Self::Closed { thread_id } => {
                let short_id =
                    &thread_id[..8.min(thread_id.len())];
                format!("📦 Archived thread {short_id}")
            }
            Self::Context {
                thread,
                message_count,
            } => {
                let short_id =
                    &thread.id[..8.min(thread.id.len())];
                format!(
                    "📎 Current thread: \"{}\"\n   ID: {}\n   Messages: {}\n   Created: {}",
                    thread.topic, short_id, message_count, thread.created_at
                )
            }
            Self::SearchResults { query, results } => {
                if results.is_empty() {
                    return format!("🔍 No results for \"{query}\"");
                }
                let mut out = format!("🔍 Results for \"{query}\":\n");
                for r in results {
                    out.push_str(&format!(
                        "  • {} ({} matches)\n",
                        r.thread_topic, r.match_count
                    ));
                }
                out
            }
            Self::Help => "🧵 Thread Commands:\n\
                 /threads          — list active threads\n\
                 /thread new [topic] — start a new thread\n\
                 /thread switch <name> — switch thread\n\
                 /thread close     — archive current thread\n\
                 /thread context   — show current thread\n\
                 /thread search <q> — search thread history\n\
                 /thread help      — this message"
                .to_string(),
            Self::Error { message } => format!("❌ {message}"),
        }
    }

    /// Format for Telegram (with potential inline keyboard data).
    pub fn format_telegram(&self) -> (String, Option<serde_json::Value>) {
        let text = self.format_text();
        let keyboard = match self {
            Self::ThreadList { threads, active_id } => {
                // Generate inline keyboard with thread buttons
                let buttons: Vec<serde_json::Value> = threads
                    .iter()
                    .filter(|t| t.id != *active_id && t.state == ThreadState::Active)
                    .take(5) // max 5 buttons
                    .map(|t| {
                        serde_json::json!({
                            "text": format!("→ {}", t.topic),
                            "callback_data": format!("/thread switch {}", t.id)
                        })
                    })
                    .collect();

                if buttons.is_empty() {
                    None
                } else {
                    Some(serde_json::json!({
                        "inline_keyboard": [buttons]
                    }))
                }
            }
            _ => None,
        };
        (text, keyboard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threading::store::MemoryThreadStore;

    // ── Parse tests ──────────────────────────────────────────────────────

    #[test]
    fn parse_threads_command() {
        assert_eq!(ThreadCommand::parse("/threads"), Some(ThreadCommand::List));
    }

    #[test]
    fn parse_thread_new_with_topic() {
        assert_eq!(
            ThreadCommand::parse("/thread new my topic"),
            Some(ThreadCommand::New {
                topic: Some("my topic".to_string())
            })
        );
    }

    #[test]
    fn parse_thread_new_no_topic() {
        assert_eq!(
            ThreadCommand::parse("/thread new"),
            Some(ThreadCommand::New { topic: None })
        );
    }

    #[test]
    fn parse_thread_switch() {
        assert_eq!(
            ThreadCommand::parse("/thread switch abc123"),
            Some(ThreadCommand::Switch {
                target: "abc123".to_string()
            })
        );
    }

    #[test]
    fn parse_thread_switch_short_alias() {
        assert_eq!(
            ThreadCommand::parse("/thread s abc"),
            Some(ThreadCommand::Switch {
                target: "abc".to_string()
            })
        );
    }

    #[test]
    fn parse_thread_close_no_target() {
        assert_eq!(
            ThreadCommand::parse("/thread close"),
            Some(ThreadCommand::Close { target: None })
        );
    }

    #[test]
    fn parse_thread_close_with_target() {
        assert_eq!(
            ThreadCommand::parse("/thread close my-thread"),
            Some(ThreadCommand::Close {
                target: Some("my-thread".to_string())
            })
        );
    }

    #[test]
    fn parse_thread_context() {
        assert_eq!(
            ThreadCommand::parse("/thread context"),
            Some(ThreadCommand::Context)
        );
    }

    #[test]
    fn parse_thread_context_alias() {
        assert_eq!(
            ThreadCommand::parse("/thread ?"),
            Some(ThreadCommand::Context)
        );
    }

    #[test]
    fn parse_thread_search() {
        assert_eq!(
            ThreadCommand::parse("/thread search hello"),
            Some(ThreadCommand::Search {
                query: "hello".to_string()
            })
        );
    }

    #[test]
    fn parse_thread_search_find_alias() {
        assert_eq!(
            ThreadCommand::parse("/thread find something"),
            Some(ThreadCommand::Search {
                query: "something".to_string()
            })
        );
    }

    #[test]
    fn parse_thread_help() {
        assert_eq!(
            ThreadCommand::parse("/thread help"),
            Some(ThreadCommand::Help)
        );
    }

    #[test]
    fn parse_thread_no_subcommand() {
        assert_eq!(ThreadCommand::parse("/thread"), Some(ThreadCommand::Help));
    }

    #[test]
    fn parse_thread_invalid_subcommand() {
        assert_eq!(
            ThreadCommand::parse("/thread foobar"),
            Some(ThreadCommand::Invalid {
                input: "foobar".to_string()
            })
        );
    }

    #[test]
    fn parse_regular_message_returns_none() {
        assert_eq!(ThreadCommand::parse("hello world"), None);
    }

    #[test]
    fn parse_other_command_returns_none() {
        assert_eq!(ThreadCommand::parse("/help"), None);
        assert_eq!(ThreadCommand::parse("/start"), None);
    }

    #[test]
    fn parse_threadnew_no_space_returns_none() {
        // "/threadnew" (no space after /thread) should NOT parse as a command
        assert_eq!(ThreadCommand::parse("/threadnew"), None);
    }

    #[test]
    fn parse_with_leading_whitespace() {
        assert_eq!(
            ThreadCommand::parse("  /threads  "),
            Some(ThreadCommand::List)
        );
    }

    #[test]
    fn parse_rename_command() {
        assert_eq!(
            ThreadCommand::parse("/thread rename new name"),
            Some(ThreadCommand::Rename {
                new_name: "new name".to_string()
            })
        );
    }

    // ── Executor tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn execute_list_no_threads() {
        let store = Arc::new(MemoryThreadStore::new());
        let executor = ThreadCommandExecutor::new(store);
        let result = executor.execute("chat-1", &ThreadCommand::List).await;

        match result {
            ThreadCommandResult::ThreadList { threads, .. } => {
                assert!(threads.is_empty());
            }
            _ => panic!("Expected ThreadList"),
        }
    }

    #[tokio::test]
    async fn execute_list_with_threads() {
        let store = Arc::new(MemoryThreadStore::new());
        store.create_thread("chat-1", "topic-a").await;
        store.create_thread("chat-1", "topic-b").await;

        let executor = ThreadCommandExecutor::new(store);
        let result = executor.execute("chat-1", &ThreadCommand::List).await;

        match result {
            ThreadCommandResult::ThreadList {
                threads,
                active_id,
            } => {
                assert_eq!(threads.len(), 2);
                // active_id should be the last created thread (topic-b)
                let active_thread = threads.iter().find(|t| t.id == active_id).unwrap();
                assert_eq!(active_thread.topic, "topic-b");
            }
            _ => panic!("Expected ThreadList"),
        }
    }

    #[tokio::test]
    async fn execute_new_creates_thread() {
        let store = Arc::new(MemoryThreadStore::new());
        let executor = ThreadCommandExecutor::new(store.clone());

        let result = executor
            .execute(
                "chat-1",
                &ThreadCommand::New {
                    topic: Some("my topic".to_string()),
                },
            )
            .await;

        match result {
            ThreadCommandResult::Created { thread } => {
                assert_eq!(thread.topic, "my topic");
            }
            _ => panic!("Expected Created"),
        }

        // Verify it's now active
        let active = store.active_thread("chat-1").await.unwrap();
        assert_eq!(active.topic, "my topic");
    }

    #[tokio::test]
    async fn execute_new_default_topic() {
        let store = Arc::new(MemoryThreadStore::new());
        let executor = ThreadCommandExecutor::new(store);

        let result = executor
            .execute("chat-1", &ThreadCommand::New { topic: None })
            .await;

        match result {
            ThreadCommandResult::Created { thread } => {
                assert_eq!(thread.topic, "untitled");
            }
            _ => panic!("Expected Created"),
        }
    }

    #[tokio::test]
    async fn execute_switch_by_topic_partial_match() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "debugging rust").await;
        store.create_thread("chat-1", "architecture").await;

        let executor = ThreadCommandExecutor::new(store.clone());
        let result = executor
            .execute(
                "chat-1",
                &ThreadCommand::Switch {
                    target: "rust".to_string(),
                },
            )
            .await;

        match result {
            ThreadCommandResult::Switched { thread } => {
                assert_eq!(thread.id, t1.id);
            }
            _ => panic!("Expected Switched"),
        }

        let active = store.active_thread("chat-1").await.unwrap();
        assert_eq!(active.id, t1.id);
    }

    #[tokio::test]
    async fn execute_switch_by_id_prefix() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "topic-a").await;
        store.create_thread("chat-1", "topic-b").await;

        let id_prefix = t1.id[..8].to_string();

        let executor = ThreadCommandExecutor::new(store.clone());
        let result = executor
            .execute(
                "chat-1",
                &ThreadCommand::Switch {
                    target: id_prefix,
                },
            )
            .await;

        match result {
            ThreadCommandResult::Switched { thread } => {
                assert_eq!(thread.id, t1.id);
            }
            _ => panic!("Expected Switched"),
        }
    }

    #[tokio::test]
    async fn execute_switch_no_match() {
        let store = Arc::new(MemoryThreadStore::new());
        store.create_thread("chat-1", "topic-a").await;

        let executor = ThreadCommandExecutor::new(store);
        let result = executor
            .execute(
                "chat-1",
                &ThreadCommand::Switch {
                    target: "nonexistent".to_string(),
                },
            )
            .await;

        match result {
            ThreadCommandResult::Error { message } => {
                assert!(message.contains("nonexistent"));
            }
            _ => panic!("Expected Error"),
        }
    }

    #[tokio::test]
    async fn execute_close_active_thread() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "topic-a").await;
        let t2 = store.create_thread("chat-1", "topic-b").await;

        // t2 is active
        let executor = ThreadCommandExecutor::new(store.clone());
        let result = executor
            .execute("chat-1", &ThreadCommand::Close { target: None })
            .await;

        match result {
            ThreadCommandResult::Closed { thread_id } => {
                assert_eq!(thread_id, t2.id);
            }
            _ => panic!("Expected Closed"),
        }
    }

    #[tokio::test]
    async fn execute_context() {
        let store = Arc::new(MemoryThreadStore::new());
        let t1 = store.create_thread("chat-1", "testing").await;
        store
            .add_message("chat-1", ChatMessage::user("hello"))
            .await;
        store
            .add_message("chat-1", ChatMessage::assistant("hi"))
            .await;

        let executor = ThreadCommandExecutor::new(store);
        let result = executor
            .execute("chat-1", &ThreadCommand::Context)
            .await;

        match result {
            ThreadCommandResult::Context {
                thread,
                message_count,
            } => {
                assert_eq!(thread.id, t1.id);
                assert_eq!(message_count, 2);
            }
            _ => panic!("Expected Context"),
        }
    }

    #[tokio::test]
    async fn execute_search_with_matches() {
        let store = Arc::new(MemoryThreadStore::new());
        store.create_thread("chat-1", "topic-a").await;
        store
            .add_message("chat-1", ChatMessage::user("hello world"))
            .await;
        store
            .add_message("chat-1", ChatMessage::user("goodbye world"))
            .await;

        store.create_thread("chat-1", "topic-b").await;
        store
            .add_message("chat-1", ChatMessage::user("something else"))
            .await;

        let executor = ThreadCommandExecutor::new(store);
        let result = executor
            .execute(
                "chat-1",
                &ThreadCommand::Search {
                    query: "world".to_string(),
                },
            )
            .await;

        match result {
            ThreadCommandResult::SearchResults { query, results } => {
                assert_eq!(query, "world");
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].thread_topic, "topic-a");
                assert_eq!(results[0].match_count, 2);
            }
            _ => panic!("Expected SearchResults"),
        }
    }

    #[tokio::test]
    async fn execute_search_no_matches() {
        let store = Arc::new(MemoryThreadStore::new());
        store.create_thread("chat-1", "topic-a").await;
        store
            .add_message("chat-1", ChatMessage::user("hello"))
            .await;

        let executor = ThreadCommandExecutor::new(store);
        let result = executor
            .execute(
                "chat-1",
                &ThreadCommand::Search {
                    query: "nonexistent".to_string(),
                },
            )
            .await;

        match result {
            ThreadCommandResult::SearchResults { results, .. } => {
                assert!(results.is_empty());
            }
            _ => panic!("Expected SearchResults"),
        }
    }

    #[tokio::test]
    async fn execute_help() {
        let store = Arc::new(MemoryThreadStore::new());
        let executor = ThreadCommandExecutor::new(store);
        let result = executor
            .execute("chat-1", &ThreadCommand::Help)
            .await;

        assert!(matches!(result, ThreadCommandResult::Help));
    }

    #[tokio::test]
    async fn execute_invalid() {
        let store = Arc::new(MemoryThreadStore::new());
        let executor = ThreadCommandExecutor::new(store);
        let result = executor
            .execute(
                "chat-1",
                &ThreadCommand::Invalid {
                    input: "xyz".to_string(),
                },
            )
            .await;

        match result {
            ThreadCommandResult::Error { message } => {
                assert!(message.contains("xyz"));
            }
            _ => panic!("Expected Error"),
        }
    }

    // ── Format tests ─────────────────────────────────────────────────────

    #[test]
    fn format_text_empty_list() {
        let result = ThreadCommandResult::ThreadList {
            threads: vec![],
            active_id: String::new(),
        };
        assert_eq!(result.format_text(), "No active threads.");
    }

    #[test]
    fn format_text_thread_list() {
        let threads = vec![
            Thread::new("t1", "chat-1", "topic-a"),
            Thread::new("t2", "chat-1", "topic-b"),
        ];
        let result = ThreadCommandResult::ThreadList {
            threads,
            active_id: "t1".to_string(),
        };
        let text = result.format_text();
        assert!(text.contains("topic-a"));
        assert!(text.contains("← active"));
        assert!(text.contains("topic-b"));
    }

    #[test]
    fn format_text_created() {
        let thread = Thread::new("t1", "chat-1", "new topic");
        let result = ThreadCommandResult::Created { thread };
        let text = result.format_text();
        assert!(text.contains("new topic"));
        assert!(text.contains("now active"));
    }

    #[test]
    fn format_text_help() {
        let result = ThreadCommandResult::Help;
        let text = result.format_text();
        assert!(text.contains("/threads"));
        assert!(text.contains("/thread new"));
        assert!(text.contains("/thread switch"));
    }

    #[test]
    fn format_text_search_no_results() {
        let result = ThreadCommandResult::SearchResults {
            query: "foobar".to_string(),
            results: vec![],
        };
        let text = result.format_text();
        assert!(text.contains("No results"));
        assert!(text.contains("foobar"));
    }

    #[test]
    fn format_text_search_with_results() {
        let result = ThreadCommandResult::SearchResults {
            query: "hello".to_string(),
            results: vec![SearchResult {
                thread_id: "t1".to_string(),
                thread_topic: "my thread".to_string(),
                match_count: 3,
            }],
        };
        let text = result.format_text();
        assert!(text.contains("my thread"));
        assert!(text.contains("3 matches"));
    }

    #[test]
    fn format_telegram_list_with_keyboard() {
        let threads = vec![
            Thread::new("t1", "chat-1", "topic-a"),
            Thread::new("t2", "chat-1", "topic-b"),
        ];
        let result = ThreadCommandResult::ThreadList {
            threads,
            active_id: "t1".to_string(),
        };
        let (text, keyboard) = result.format_telegram();
        assert!(text.contains("topic-a"));
        assert!(keyboard.is_some());

        let kb = keyboard.unwrap();
        let inline = kb.get("inline_keyboard").unwrap().as_array().unwrap();
        assert!(!inline.is_empty());
    }

    #[test]
    fn format_telegram_no_keyboard_when_all_active() {
        // When the only thread is the active one, no keyboard buttons
        let threads = vec![Thread::new("t1", "chat-1", "only-thread")];
        let result = ThreadCommandResult::ThreadList {
            threads,
            active_id: "t1".to_string(),
        };
        let (_, keyboard) = result.format_telegram();
        assert!(keyboard.is_none());
    }

    #[test]
    fn format_text_error() {
        let result = ThreadCommandResult::Error {
            message: "something went wrong".to_string(),
        };
        let text = result.format_text();
        assert!(text.contains("❌"));
        assert!(text.contains("something went wrong"));
    }
}