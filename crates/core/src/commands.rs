//! Shared slash command registry for all channel adapters (TUI, Telegram, etc.)

use chrono::Utc;

/// Result of executing a command.
pub enum CommandResult {
    /// Display this text to the user.
    Response(String),
    /// Clear the chat history.
    ClearHistory,
    /// Quit the application.
    Quit,
    /// Not a command — pass through to the agent.
    NotACommand,
    /// Switch model.
    SwitchModel(String),
    /// Session management commands.
    Session(SessionCommand),
}

/// Session management sub-commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
    /// List all sessions for the current chat.
    List,
    /// Create a new named session (archives the current one).
    New(String),
    /// Switch to an existing session by name or key.
    Switch(String),
    /// Archive the current session.
    Archive,
}

/// Shared command definitions.
pub struct CommandRegistry;

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self
    }

    /// Execute a slash command. Returns CommandResult.
    pub fn execute(&self, input: &str, context: &CommandContext) -> CommandResult {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return CommandResult::NotACommand;
        }

        let (cmd, args) = match trimmed.split_once(' ') {
            Some((c, a)) => (c, a.trim()),
            None => (trimmed, ""),
        };

        match cmd {
            "/quit" | "/exit" | "/q" => CommandResult::Quit,
            "/clear" => CommandResult::ClearHistory,
            "/help" => CommandResult::Response(self.help_text()),
            "/status" => CommandResult::Response(self.status_text(context)),
            "/model" => {
                if args.is_empty() {
                    CommandResult::Response(format!(
                        "Primary: {}\nDeep: {}",
                        context.primary_model, context.deep_model
                    ))
                } else {
                    CommandResult::SwitchModel(args.to_string())
                }
            }
            "/version" => CommandResult::Response(format!(
                "pares-radix v{}\nBuild: {}",
                env!("CARGO_PKG_VERSION"),
                option_env!("GIT_HASH").unwrap_or("dev")
            )),
            "/session" => self.handle_session_command(args),
            "/memory" => {
                if args.is_empty() {
                    CommandResult::Response("Usage: /memory <query> — or just ask the agent, it searches memory automatically.".into())
                } else {
                    CommandResult::NotACommand // Let it pass through to the agent for PluresDB search
                }
            }
            "/tools" => CommandResult::Response(
                "Tool governance: all tools require explicit user approval.\n\
                 Registered: read_file, write_file, edit_file, list_dir, run_command, web_fetch"
                    .into(),
            ),
            "/config" => CommandResult::Response(format!(
                "Model: {}\nDeep: {}\nEndpoint: {}\nLog level: info",
                context.primary_model, context.deep_model, context.endpoint
            )),
            _ => CommandResult::Response(format!(
                "Unknown command: {cmd}. Type /help for available commands."
            )),
        }
    }

    fn handle_session_command(&self, args: &str) -> CommandResult {
        let (sub, sub_args) = match args.split_once(' ') {
            Some((s, a)) => (s.trim(), a.trim()),
            None => (args.trim(), ""),
        };

        match sub {
            "" | "list" => CommandResult::Session(SessionCommand::List),
            "new" => {
                let name = if sub_args.is_empty() {
                    Utc::now().format("%Y%m%d-%H%M%S").to_string()
                } else {
                    sub_args.to_string()
                };
                CommandResult::Session(SessionCommand::New(name))
            }
            "switch" => {
                if sub_args.is_empty() {
                    CommandResult::Response(
                        "Usage: /session switch <name> — switch to a named session".into(),
                    )
                } else {
                    CommandResult::Session(SessionCommand::Switch(sub_args.to_string()))
                }
            }
            "archive" => CommandResult::Session(SessionCommand::Archive),
            _ => CommandResult::Response(format!(
                "Unknown session command: {sub}\n\
                 Usage: /session [list|new [name]|switch <name>|archive]"
            )),
        }
    }

    fn help_text(&self) -> String {
        "Commands:\n  \
         /model [name]    — show or switch primary model\n  \
         /session [cmd]   — list/new/switch/archive sessions\n  \
         /config          — show runtime config\n  \
         /status          — show session status\n  \
         /memory <query>  — search PluresDB memories\n  \
         /tools           — show tool governance\n  \
         /version         — show version info\n  \
         /clear           — clear conversation\n  \
         /quit            — exit\n  \
         /help            — this message"
            .into()
    }

    fn status_text(&self, ctx: &CommandContext) -> String {
        format!(
            "Model: {}\nDeep: {}\nMessages: {}\nMemory entries: {}",
            ctx.primary_model, ctx.deep_model, ctx.message_count, ctx.memory_count
        )
    }
}

/// Context passed to command execution.
pub struct CommandContext {
    pub primary_model: String,
    pub deep_model: String,
    pub endpoint: String,
    pub message_count: usize,
    pub memory_count: usize,
}
