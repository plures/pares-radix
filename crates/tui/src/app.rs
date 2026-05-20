//! TUI application state and event loop.

use std::sync::Arc;
use tokio::sync::mpsc;

use pares_agens_core::agent::Agent;
use pares_agens_core::commands::{CommandContext, CommandRegistry, CommandResult};
use pares_agens_core::model::StreamDelta;
use pares_agens_core::Event;

/// A single chat message displayed in the TUI.
#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
}

/// Events the TUI event loop processes.
pub enum AppEvent {
    /// User submitted input text.
    UserInput(String),
    /// A streaming chunk arrived from the model.
    StreamChunk(String),
    /// Agent finished responding (final complete content).
    AgentResponse(String),
    /// Terminal resize or redraw needed.
    Redraw,
    /// Quit the application.
    Quit,
}

/// Application state.
pub struct App {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub input_cursor: usize,
    pub scroll_offset: u16,
    pub user_scrolled: bool,
    pub viewport_height: u16,
    pub thinking: bool,
    /// True while streaming chunks are arriving (after first chunk, before final response).
    pub streaming: bool,
    pub current_model: String,
    pub agent: Arc<Agent>,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
}

impl App {
    pub fn new(
        agent: Arc<Agent>,
        model_name: String,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Self {
        Self {
            messages: vec![ChatMessage {
                role: Role::System,
                content: format!("Pares Radix TUI — model: {model_name}. Type /help for commands."),
                timestamp: chrono::Utc::now(),
            }],
            input: String::new(),
            input_cursor: 0,
            scroll_offset: 0,
            user_scrolled: false,
            viewport_height: 35,
            thinking: false,
            streaming: false,
            current_model: model_name,
            agent,
            event_tx,
        }
    }

    /// Handle a slash command. Returns true if it was a command.
    pub fn handle_command(&mut self, input: &str) -> bool {
        let registry = CommandRegistry::new();
        let ctx = CommandContext {
            primary_model: self.current_model.clone(),
            deep_model: String::from("claude-opus-4.6"),
            endpoint: String::from("copilot"),
            message_count: self.messages.len(),
            memory_count: 0,
        };
        match registry.execute(input, &ctx) {
            CommandResult::NotACommand => false,
            CommandResult::Response(text) => {
                self.push_system(&text);
                true
            }
            CommandResult::ClearHistory => {
                self.messages.clear();
                self.scroll_offset = 0;
                self.push_system("Chat cleared.");
                true
            }
            CommandResult::Quit => {
                let _ = self.event_tx.send(AppEvent::Quit);
                true
            }
            CommandResult::SwitchModel(name) => {
                self.current_model = name.clone();
                self.push_system(&format!("Model switched to: {name}"));
                true
            }
        }
    }

    /// Submit the current input buffer.
    pub fn submit_input(&mut self) {
        let input = self.input.drain(..).collect::<String>();
        self.input_cursor = 0;
        let trimmed = input.trim().to_string();
        if trimmed.is_empty() {
            return;
        }

        if self.handle_command(&trimmed) {
            return;
        }

        // Add user message
        self.messages.push(ChatMessage {
            role: Role::User,
            content: trimmed.clone(),
            timestamp: chrono::Utc::now(),
        });
        self.scroll_to_bottom();
        self.thinking = true;

        // Spawn agent call with streaming
        let agent = Arc::clone(&self.agent);
        let tx = self.event_tx.clone();
        let handle = tokio::spawn(async move {
            let event = Event::Message {
                id: uuid::Uuid::new_v4().to_string(),
                channel: "tui".into(),
                sender: "user".into(),
                content: trimmed,
            };

            // Create streaming channel
            let (stream_tx, mut stream_rx) = mpsc::unbounded_channel::<StreamDelta>();

            // Forward stream deltas to the TUI event loop as they arrive
            let chunk_tx = tx.clone();
            let forwarder = tokio::spawn(async move {
                while let Some(delta) = stream_rx.recv().await {
                    match delta {
                        StreamDelta::Content(content) => {
                            let _ = chunk_tx.send(AppEvent::StreamChunk(content));
                        }
                        StreamDelta::Done => break,
                        _ => {} // ToolCallStart/Delta handled internally by the agent
                    }
                }
            });

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(120),
                agent.handle_event_streaming(event, stream_tx),
            )
            .await;

            // Ensure forwarder completes
            let _ = forwarder.await;

            match result {
                Ok(Some(Event::ModelResponse { content, .. })) => content,
                Ok(Some(_other)) => "(unexpected response type)".to_string(),
                Ok(None) => {
                    "(agent returned no response — check ~/.pares-radix/logs/pares-radix.log)"
                        .to_string()
                }
                Err(_timeout) => "(timed out after 120s)".to_string(),
            }
        });
        // Spawn a watcher that catches panics from the agent task
        let tx2 = self.event_tx.clone();
        tokio::spawn(async move {
            match handle.await {
                Ok(content) => {
                    let _ = tx2.send(AppEvent::AgentResponse(content));
                }
                Err(join_err) => {
                    let msg = if join_err.is_panic() {
                        "(internal error — agent panicked, check logs)".to_string()
                    } else {
                        "(agent task cancelled)".to_string()
                    };
                    let _ = tx2.send(AppEvent::AgentResponse(msg));
                }
            }
        });
    }

    pub fn push_system(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            role: Role::System,
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
        });
    }

    /// Handle a streaming chunk: append to the current assistant message or create one.
    pub fn handle_stream_chunk(&mut self, chunk: String) {
        // If we're in thinking state and no assistant message is being built yet, create one.
        if self.thinking {
            if let Some(last) = self.messages.last_mut() {
                if last.role == Role::Assistant {
                    // Append to in-progress assistant message
                    last.content.push_str(&chunk);
                    self.scroll_to_bottom();
                    return;
                }
            }
            // First chunk — create the in-progress assistant message and stop showing spinner
            self.thinking = false;
            self.streaming = true;
            self.messages.push(ChatMessage {
                role: Role::Assistant,
                content: chunk,
                timestamp: chrono::Utc::now(),
            });
            self.scroll_to_bottom();
        } else if let Some(last) = self.messages.last_mut() {
            if last.role == Role::Assistant {
                last.content.push_str(&chunk);
                self.scroll_to_bottom();
            }
        }
    }

    /// Handle the final agent response — replace streaming content with final canonical content.
    pub fn handle_agent_response(&mut self, content: String) {
        self.thinking = false;
        self.streaming = false;
        // If the last message is an in-progress assistant message from streaming,
        // replace it with the final canonical response.
        if let Some(last) = self.messages.last_mut() {
            if last.role == Role::Assistant {
                last.content = content;
                self.scroll_to_bottom();
                return;
            }
        }
        // No streaming message was created (e.g., timeout error) — add new one.
        self.messages.push(ChatMessage {
            role: Role::Assistant,
            content,
            timestamp: chrono::Utc::now(),
        });
        self.scroll_to_bottom();
    }

    /// Auto-scroll to show the latest message (respects user scroll position).
    pub fn scroll_to_bottom(&mut self) {
        if self.user_scrolled {
            return; // Don't auto-scroll if user manually scrolled up
        }
        let total_lines: u16 = self
            .messages
            .iter()
            .map(|m| m.content.lines().count() as u16 + 2)
            .sum::<u16>()
            + 2;
        self.scroll_offset = total_lines.saturating_sub(self.viewport_height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pares_agens_core::agent::Agent;
    use pares_agens_core::model::StreamDelta;

    /// Create a minimal App for testing (uses a mock agent).
    fn test_app() -> (App, mpsc::UnboundedReceiver<AppEvent>) {
        use pares_agens_core::agent::Memory;
        use pares_agens_core::model::{
            ChatMessage as CoreChatMessage, ChatOptions, ModelClient, ModelCompletion,
            StreamSender, ToolDefinition, ToolDispatcher,
        };
        use async_trait::async_trait;
        use serde_json::Value;

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

        let agent = Arc::new(
            Agent::new(Arc::new(NoopMemory))
                .with_model(
                    Arc::new(NoopModel),
                    Arc::new(NoopDispatcher),
                    "test".to_string(),
                ),
        );

        let (tx, rx) = mpsc::unbounded_channel();
        let app = App::new(agent, "test-model".to_string(), tx);
        (app, rx)
    }

    #[test]
    fn stream_chunk_creates_assistant_message() {
        let (mut app, _rx) = test_app();
        app.thinking = true;

        app.handle_stream_chunk("Hello".to_string());

        // Should have created an assistant message, cleared thinking, and set streaming
        assert!(!app.thinking);
        assert!(app.streaming);
        let last = app.messages.last().unwrap();
        assert_eq!(last.role, Role::Assistant);
        assert_eq!(last.content, "Hello");
    }

    #[test]
    fn stream_chunk_appends_to_existing_assistant() {
        let (mut app, _rx) = test_app();
        app.thinking = true;

        app.handle_stream_chunk("Hello".to_string());
        app.handle_stream_chunk(" world".to_string());

        let last = app.messages.last().unwrap();
        assert_eq!(last.content, "Hello world");
        assert!(app.streaming);
    }

    #[test]
    fn agent_response_replaces_streaming_content() {
        let (mut app, _rx) = test_app();
        app.thinking = true;

        // Simulate streaming
        app.handle_stream_chunk("Hell".to_string());
        app.handle_stream_chunk("o world".to_string());
        assert!(app.streaming);

        // Then final response arrives (canonical, may differ from streamed)
        app.handle_agent_response("Hello world!".to_string());

        // Should have exactly one assistant message with final content, streaming off
        assert!(!app.streaming);
        let assistant_msgs: Vec<_> = app
            .messages
            .iter()
            .filter(|m| m.role == Role::Assistant)
            .collect();
        assert_eq!(assistant_msgs.len(), 1);
        assert_eq!(assistant_msgs[0].content, "Hello world!");
    }

    #[test]
    fn agent_response_without_streaming_creates_message() {
        let (mut app, _rx) = test_app();
        app.thinking = true;

        // No stream chunks — direct response (e.g., procedural route)
        app.handle_agent_response("Direct answer".to_string());

        let last = app.messages.last().unwrap();
        assert_eq!(last.role, Role::Assistant);
        assert_eq!(last.content, "Direct answer");
        assert!(!app.thinking);
        assert!(!app.streaming);
    }
}
