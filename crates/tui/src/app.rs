//! TUI application state and event loop.

use std::sync::Arc;
use tokio::sync::mpsc;

use pares_agens_core::agent::Agent;
use pares_agens_core::Event;
use pares_agens_core::commands::{CommandRegistry, CommandContext, CommandResult};

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
    /// Agent finished responding.
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
    pub thinking: bool,
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
                content: format!("Pares Agens TUI — model: {model_name}. Type /help for commands."),
                timestamp: chrono::Utc::now(),
            }],
            input: String::new(),
            input_cursor: 0,
            scroll_offset: 0,
            thinking: false,
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
            CommandResult::Response(text) => { self.push_system(&text); true }
            CommandResult::ClearHistory => {
                self.messages.clear();
                self.scroll_offset = 0;
                self.push_system("Chat cleared.");
                true
            }
            CommandResult::Quit => { let _ = self.event_tx.send(AppEvent::Quit); true }
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

        // Spawn agent call with timeout
        let agent = Arc::clone(&self.agent);
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            let event = Event::Message {
                id: uuid::Uuid::new_v4().to_string(),
                channel: "tui".into(),
                sender: "user".into(),
                content: trimmed,
            };
            match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                agent.handle_event(event),
            ).await {
                Ok(Some(Event::ModelResponse { content, .. })) => {
                    let _ = tx.send(AppEvent::AgentResponse(content));
                }
                Ok(Some(_other)) => {
                    let _ = tx.send(AppEvent::AgentResponse(
                        "(unexpected response type from agent)".to_string(),
                    ));
                }
                Ok(None) => {
                    let _ = tx.send(AppEvent::AgentResponse(
                        "(agent returned no response — check ~/.pares-agens/logs/pares-radix.log)".to_string(),
                    ));
                }
                Err(_timeout) => {
                    let _ = tx.send(AppEvent::AgentResponse(
                        "(request timed out after 30s — check ~/.pares-agens/logs/pares-radix.log)".to_string(),
                    ));
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

    pub fn handle_agent_response(&mut self, content: String) {
        self.thinking = false;
        self.messages.push(ChatMessage {
            role: Role::Assistant,
            content,
            timestamp: chrono::Utc::now(),
        });
        self.scroll_to_bottom();
    }

    /// Auto-scroll to show the latest message.
    pub fn scroll_to_bottom(&mut self) {
        let total_lines: u16 = self.messages.iter().map(|m| {
            m.content.lines().count() as u16 + 1
        }).sum::<u16>() + 2;
        self.scroll_offset = total_lines.saturating_sub(20);
    }
}
