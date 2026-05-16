//! Shared slash command registry for all channel adapters (TUI, Telegram, etc.)

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

    fn help_text(&self) -> String {
        "Commands:\n  \
         /model [name]    — show or switch primary model\n  \
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
