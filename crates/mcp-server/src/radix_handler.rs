//! Radix tool handler — wires MCP server tool calls to real pares-radix tools.
//!
//! This is the production `ToolHandler` implementation that connects incoming
//! MCP `tools/call` requests to the actual tool implementations:
//! - File I/O (read, write, edit, list_directory)
//! - Shell execution (run_command, process)
//! - Memory (memory_search, memory_store)
//! - Web (web_fetch, web_search)
//! - Cron (cron_list, cron_add, cron_remove, cron_toggle)
//! - State/DB (db_get, db_put, db_delete)

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{debug, warn};

use mcp_client::protocol::{Tool, ToolInputSchema};
use pares_agens_core::memory::PluresLm;
use pares_agens_core::shell_executor::ShellExecutor;
use pares_agens_core::StateStore;

use pares_agens_agenda::scheduler::Scheduler;

use crate::handler::{ToolHandler, ToolResult};

/// Production tool handler that connects MCP tool calls to real implementations.
pub struct RadixToolHandler {
    /// Shell executor for run_command/process tools.
    shell: Arc<ShellExecutor>,
    /// Memory system for semantic search/store.
    memory: Option<Arc<PluresLm>>,
    /// Scheduler for cron tools.
    scheduler: Option<Arc<Scheduler>>,
    /// Key-value state store for db_get/db_put/db_delete tools.
    state_store: Option<Arc<dyn StateStore>>,
    /// Working directory for file operations.
    workdir: PathBuf,
    /// Brave Search API key (optional).
    brave_api_key: Option<String>,
}

impl RadixToolHandler {
    /// Create a new handler with required dependencies.
    pub fn new(shell: Arc<ShellExecutor>, workdir: PathBuf) -> Self {
        Self {
            shell,
            memory: None,
            scheduler: None,
            state_store: None,
            workdir,
            brave_api_key: None,
        }
    }

    /// Attach a memory system for memory_search/memory_store tools.
    pub fn with_memory(mut self, memory: Arc<PluresLm>) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Attach a scheduler for cron tools.
    pub fn with_scheduler(mut self, scheduler: Arc<Scheduler>) -> Self {
        self.scheduler = Some(scheduler);
        self
    }

    /// Set the Brave Search API key for web_search.
    pub fn with_brave_api_key(mut self, key: String) -> Self {
        self.brave_api_key = Some(key);
        self
    }

    /// Attach a state store for db_get/db_put/db_delete tools.
    pub fn with_state_store(mut self, store: Arc<dyn StateStore>) -> Self {
        self.state_store = Some(store);
        self
    }

    // ── File tools ────────────────────────────────────────────────────────────

    async fn read_file(&self, args: &Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => self.resolve_path(p),
            None => return ToolResult::error("missing required parameter: path"),
        };

        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                // Truncate to 50KB like OpenClaw
                let truncated = if content.len() > 50_000 {
                    format!(
                        "{}\n\n... [truncated, {} total bytes]",
                        &content[..50_000],
                        content.len()
                    )
                } else {
                    content
                };
                ToolResult::ok(truncated)
            }
            Err(e) => ToolResult::error(format!("failed to read {}: {e}", path.display())),
        }
    }

    async fn write_file(&self, args: &Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => self.resolve_path(p),
            None => return ToolResult::error("missing required parameter: path"),
        };
        let content = match args.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("missing required parameter: content"),
        };

        // Create parent directories
        if let Some(parent) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return ToolResult::error(format!(
                    "failed to create directories for {}: {e}",
                    path.display()
                ));
            }
        }

        match tokio::fs::write(&path, content).await {
            Ok(()) => ToolResult::ok(format!("wrote {} bytes to {}", content.len(), path.display())),
            Err(e) => ToolResult::error(format!("failed to write {}: {e}", path.display())),
        }
    }

    async fn edit_file(&self, args: &Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => self.resolve_path(p),
            None => return ToolResult::error("missing required parameter: path"),
        };
        let old_text = match args.get("old_text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("missing required parameter: old_text"),
        };
        let new_text = match args.get("new_text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("missing required parameter: new_text"),
        };

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return ToolResult::error(format!("failed to read {}: {e}", path.display())),
        };

        if !content.contains(old_text) {
            return ToolResult::error(format!(
                "old_text not found in {}",
                path.display()
            ));
        }

        let new_content = content.replacen(old_text, new_text, 1);
        match tokio::fs::write(&path, &new_content).await {
            Ok(()) => ToolResult::ok(format!("edited {}", path.display())),
            Err(e) => ToolResult::error(format!("failed to write {}: {e}", path.display())),
        }
    }

    async fn list_directory(&self, args: &Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => self.resolve_path(p),
            None => return ToolResult::error("missing required parameter: path"),
        };

        let mut entries = match tokio::fs::read_dir(&path).await {
            Ok(rd) => rd,
            Err(e) => {
                return ToolResult::error(format!(
                    "failed to read directory {}: {e}",
                    path.display()
                ))
            }
        };

        let mut items = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry
                .file_type()
                .await
                .map(|ft| ft.is_dir())
                .unwrap_or(false);
            if is_dir {
                items.push(format!("{name}/"));
            } else {
                items.push(name);
            }
        }
        items.sort();
        ToolResult::ok(items.join("\n"))
    }

    // ── Shell tools ───────────────────────────────────────────────────────────

    async fn run_command(&self, args: &Value) -> ToolResult {
        use pares_agens_core::shell_executor::ExecRequest;

        let command = match args.get("command").and_then(|v| v.as_str()) {
            Some(c) => c.to_string(),
            None => return ToolResult::error("missing required parameter: command"),
        };

        let workdir = args
            .get("workdir")
            .and_then(|v| v.as_str())
            .map(|p| self.resolve_path(p).to_string_lossy().to_string())
            .or_else(|| Some(self.workdir.to_string_lossy().to_string()));

        let background = args.get("background").and_then(|v| v.as_bool()).unwrap_or(false);
        let pty = args.get("pty").and_then(|v| v.as_bool()).unwrap_or(false);
        let timeout_secs = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30);

        let request = ExecRequest {
            command,
            workdir,
            background,
            pty,
            timeout_secs: Some(timeout_secs),
            yield_ms: args.get("yieldMs").and_then(|v| v.as_u64()),
            env: args
                .get("env")
                .and_then(|v| v.as_object())
                .map(|obj| {
                    obj.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default(),
        };

        let result = self.shell.exec(request).await;
        if background {
            ToolResult::ok(format!(
                "Background session started: {}",
                result.session_id.unwrap_or_default()
            ))
        } else {
            let output = if result.exit_code == Some(0) {
                result.stdout
            } else {
                format!(
                    "exit code: {}\nstdout: {}\nstderr: {}",
                    result.exit_code.unwrap_or(-1),
                    result.stdout,
                    result.stderr
                )
            };
            ToolResult::ok(output)
        }
    }

    async fn process_action(&self, args: &Value) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::error("missing required parameter: action"),
        };

        match action {
            "list" => {
                let sessions = self.shell.list().await;
                let json_out = serde_json::to_string_pretty(&sessions).unwrap_or_default();
                ToolResult::ok(json_out)
            }
            "poll" => {
                let session_id = match args.get("sessionId").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => return ToolResult::error("missing sessionId for poll"),
                };
                let timeout_ms = args.get("timeout").and_then(|v| v.as_u64());
                match self.shell.poll(session_id, timeout_ms).await {
                    Some(info) => {
                        let json_out = serde_json::to_string_pretty(&info).unwrap_or_default();
                        ToolResult::ok(json_out)
                    }
                    None => ToolResult::error(format!("session not found: {session_id}")),
                }
            }
            "kill" => {
                let session_id = match args.get("sessionId").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => return ToolResult::error("missing sessionId for kill"),
                };
                match self.shell.kill(session_id).await {
                    Ok(()) => ToolResult::ok("killed"),
                    Err(e) => ToolResult::error(format!("kill failed: {e}")),
                }
            }
            "write" => {
                let session_id = match args.get("sessionId").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => return ToolResult::error("missing sessionId for write"),
                };
                let data = args.get("data").and_then(|v| v.as_str()).unwrap_or("");
                match self.shell.write_stdin(session_id, data).await {
                    Ok(()) => ToolResult::ok("written"),
                    Err(e) => ToolResult::error(format!("write failed: {e}")),
                }
            }
            other => ToolResult::error(format!("unknown process action: {other}")),
        }
    }

    // ── Memory tools ──────────────────────────────────────────────────────────

    async fn memory_search(&self, args: &Value) -> ToolResult {
        let memory = match &self.memory {
            Some(m) => m,
            None => return ToolResult::error("memory system not configured"),
        };

        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::error("missing required parameter: query"),
        };

        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

        match memory.recall(query, limit, &[]).await {
            Ok(results) => {
                let formatted: Vec<Value> = results
                    .into_iter()
                    .map(|r| {
                        json!({
                            "content": r.content,
                            "score": r.score,
                            "tags": r.tags,
                        })
                    })
                    .collect();
                ToolResult::ok(serde_json::to_string_pretty(&formatted).unwrap_or_default())
            }
            Err(e) => ToolResult::error(format!("memory search failed: {e}")),
        }
    }

    async fn memory_store(&self, args: &Value) -> ToolResult {
        let memory = match &self.memory {
            Some(m) => m,
            None => return ToolResult::error("memory system not configured"),
        };

        let content = match args.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("missing required parameter: content"),
        };

        let tags: Vec<String> = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        match memory.capture_fact(content, tags).await {
            Ok(Some(id)) => ToolResult::ok(format!("stored memory: {id}")),
            Ok(None) => ToolResult::ok("content rejected by quality gate (too short, duplicate, or noise)"),
            Err(e) => ToolResult::error(format!("memory store failed: {e}")),
        }
    }

    // ── Web tools ─────────────────────────────────────────────────────────────

    async fn web_fetch(&self, args: &Value) -> ToolResult {
        let url = match args.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return ToolResult::error("missing required parameter: url"),
        };

        let max_chars = args
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(30_000) as usize;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        match client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    return ToolResult::error(format!("HTTP {status} for {url}"));
                }
                match resp.text().await {
                    Ok(body) => {
                        // Simple HTML to text extraction
                        let text = if body.contains("<html") || body.contains("<HTML") {
                            html2text::from_read(body.as_bytes(), 120)
                                .unwrap_or_else(|_| body.clone())
                        } else {
                            body
                        };
                        let truncated = if text.len() > max_chars {
                            format!("{}\n\n[truncated at {max_chars} chars]", &text[..max_chars])
                        } else {
                            text
                        };
                        ToolResult::ok(truncated)
                    }
                    Err(e) => ToolResult::error(format!("failed to read response body: {e}")),
                }
            }
            Err(e) => ToolResult::error(format!("fetch failed: {e}")),
        }
    }

    async fn web_search(&self, args: &Value) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::error("missing required parameter: query"),
        };

        let api_key = match &self.brave_api_key {
            Some(k) => k.clone(),
            None => return ToolResult::error("Brave Search API key not configured"),
        };

        let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(5);
        let url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={}&count={count}",
            urlencoding::encode(query)
        );

        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .header("X-Subscription-Token", &api_key)
            .header("Accept", "application/json")
            .send()
            .await;

        match resp {
            Ok(r) => match r.json::<Value>().await {
                Ok(data) => {
                    let results = data
                        .get("web")
                        .and_then(|w| w.get("results"))
                        .and_then(|r| r.as_array())
                        .cloned()
                        .unwrap_or_default();

                    let formatted: Vec<Value> = results
                        .into_iter()
                        .take(count as usize)
                        .map(|r| {
                            json!({
                                "title": r.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                                "url": r.get("url").and_then(|v| v.as_str()).unwrap_or(""),
                                "description": r.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                            })
                        })
                        .collect();

                    ToolResult::ok(
                        serde_json::to_string_pretty(&formatted).unwrap_or_default(),
                    )
                }
                Err(e) => ToolResult::error(format!("failed to parse search results: {e}")),
            },
            Err(e) => ToolResult::error(format!("search request failed: {e}")),
        }
    }

    // ── Cron tools ────────────────────────────────────────────────────────────

    async fn cron_list(&self, _args: &Value) -> ToolResult {
        let scheduler = match &self.scheduler {
            Some(s) => s,
            None => return ToolResult::error("scheduler not configured"),
        };

        let tasks = scheduler.list().await;
        let formatted: Vec<Value> = tasks
            .into_iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "name": t.name,
                    "schedule": t.schedule,
                    "command": t.command,
                    "enabled": t.enabled,
                    "last_run": t.last_run.map(|dt| dt.to_rfc3339()),
                    "last_result": t.last_result,
                })
            })
            .collect();
        ToolResult::ok(serde_json::to_string_pretty(&formatted).unwrap_or_default())
    }

    async fn cron_add(&self, args: &Value) -> ToolResult {
        use pares_agens_agenda::scheduler::{Task, Schedule};

        let scheduler = match &self.scheduler {
            Some(s) => s,
            None => return ToolResult::error("scheduler not configured"),
        };

        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => return ToolResult::error("missing required parameter: name"),
        };
        let command = match args.get("command").and_then(|v| v.as_str()) {
            Some(c) => c.to_string(),
            None => return ToolResult::error("missing required parameter: command"),
        };

        let schedule = if let Some(expr) = args.get("cron").and_then(|v| v.as_str()) {
            Schedule::Cron { expr: expr.to_string() }
        } else if let Some(secs) = args.get("interval_secs").and_then(|v| v.as_u64()) {
            Schedule::Interval { every_secs: secs }
        } else {
            return ToolResult::error("missing schedule: provide 'cron' or 'interval_secs'");
        };

        let task = Task {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            schedule,
            command,
            enabled: true,
            last_run: None,
            last_result: None,
        };

        let id = task.id.clone();
        scheduler.add(task).await;
        ToolResult::ok(format!("added task: {id}"))
    }

    async fn cron_remove(&self, args: &Value) -> ToolResult {
        let scheduler = match &self.scheduler {
            Some(s) => s,
            None => return ToolResult::error("scheduler not configured"),
        };

        let id = match args.get("id").and_then(|v| v.as_str()) {
            Some(i) => i,
            None => return ToolResult::error("missing required parameter: id"),
        };

        if scheduler.remove(id).await {
            ToolResult::ok(format!("removed task: {id}"))
        } else {
            ToolResult::error(format!("task not found: {id}"))
        }
    }

    async fn cron_toggle(&self, args: &Value) -> ToolResult {
        let scheduler = match &self.scheduler {
            Some(s) => s,
            None => return ToolResult::error("scheduler not configured"),
        };

        let id = match args.get("id").and_then(|v| v.as_str()) {
            Some(i) => i,
            None => return ToolResult::error("missing required parameter: id"),
        };
        let enabled = match args.get("enabled").and_then(|v| v.as_bool()) {
            Some(e) => e,
            None => return ToolResult::error("missing required parameter: enabled"),
        };

        if scheduler.set_enabled(id, enabled).await {
            ToolResult::ok(format!("task {id} enabled={enabled}"))
        } else {
            ToolResult::error(format!("task not found: {id}"))
        }
    }

    // ── State/DB tools ────────────────────────────────────────────────────────

    async fn db_get(&self, args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("state store not configured"),
        };

        let key = match args.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return ToolResult::error("missing required parameter: key"),
        };

        match store.get(key).await {
            Some(Value::Null) | None => ToolResult::ok("null"),
            Some(value) => ToolResult::ok(serde_json::to_string_pretty(&value).unwrap_or_default()),
        }
    }

    async fn db_put(&self, args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("state store not configured"),
        };

        let key = match args.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return ToolResult::error("missing required parameter: key"),
        };

        let value = match args.get("value") {
            Some(v) => v.clone(),
            None => return ToolResult::error("missing required parameter: value"),
        };

        store.set(key, value).await;
        ToolResult::ok(format!("stored key: {key}"))
    }

    async fn db_delete(&self, args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("state store not configured"),
        };

        let key = match args.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return ToolResult::error("missing required parameter: key"),
        };

        // Delete by writing null — StateStore doesn't have a native delete,
        // so we use the JSON null convention.
        store.set(key, Value::Null).await;
        ToolResult::ok(format!("deleted key: {key}"))
    }

    // ── Config & Runtime tools ──────────────────────────────────────────────────

    async fn config_get(&self, args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("state store not configured"),
        };

        let key = match args.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return ToolResult::error("missing required parameter: key"),
        };

        let full_key = format!("config:{key}");
        match store.get(&full_key).await {
            Some(Value::Null) | None => ToolResult::ok("null"),
            Some(value) => ToolResult::ok(serde_json::to_string_pretty(&value).unwrap_or_default()),
        }
    }

    async fn config_set(&self, args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("state store not configured"),
        };

        let key = match args.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return ToolResult::error("missing required parameter: key"),
        };

        let value = match args.get("value") {
            Some(v) => v.clone(),
            None => return ToolResult::error("missing required parameter: value"),
        };

        let full_key = format!("config:{key}");
        store.set(&full_key, value).await;

        // Update the last-modified timestamp for hot-reload detection
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        store.set("config:__last_modified", json!(now)).await;

        ToolResult::ok(format!("config set: {key}"))
    }

    async fn config_list(&self, args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("state store not configured"),
        };

        let prefix = args
            .get("prefix")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Dynamically scan all config keys from the store
        let config_prefix = format!("config:{prefix}");
        let keys = store.keys_with_prefix(&config_prefix).await;

        let mut results = json!({});
        for full_key in &keys {
            // Strip the "config:" prefix to get the user-facing key
            let user_key = full_key.strip_prefix("config:").unwrap_or(full_key);
            // Skip internal keys
            if user_key.starts_with("__") {
                continue;
            }
            if let Some(val) = store.get(full_key).await {
                if val != Value::Null {
                    results[user_key] = val;
                }
            }
        }

        ToolResult::ok(serde_json::to_string_pretty(&results).unwrap_or_default())
    }

    async fn config_delete(&self, args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("state store not configured"),
        };

        let key = match args.get("key").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return ToolResult::error("missing required parameter: key"),
        };

        let full_key = format!("config:{key}");
        let prev = store.delete(&full_key).await;

        // Update the last-modified timestamp for hot-reload detection
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        store.set("config:__last_modified", json!(now)).await;

        match prev {
            Some(v) if v != Value::Null => ToolResult::ok(format!("deleted config: {key} (was: {v})")),
            _ => ToolResult::ok(format!("config key not found: {key}")),
        }
    }

    async fn runtime_status(&self, _args: &Value) -> ToolResult {
        let uptime_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Get active shell session count
        let shell_sessions = self.shell.list().await;
        let active_count = shell_sessions.len();

        let mut status = json!({
            "status": "running",
            "version": env!("CARGO_PKG_VERSION"),
            "workdir": self.workdir.display().to_string(),
            "timestamp_unix": uptime_secs,
            "components": {
                "shell": "active",
                "memory": if self.memory.is_some() { "active" } else { "not_configured" },
                "scheduler": if self.scheduler.is_some() { "active" } else { "not_configured" },
                "state_store": if self.state_store.is_some() { "active" } else { "not_configured" },
                "web_search": if self.brave_api_key.is_some() { "active" } else { "not_configured" }
            },
            "active_shell_sessions": active_count
        });

        // Include config key count if state store is available
        if let Some(store) = &self.state_store {
            let config_keys = store.keys_with_prefix("config:").await;
            let user_keys: Vec<_> = config_keys
                .iter()
                .filter_map(|k| k.strip_prefix("config:"))
                .filter(|k| !k.starts_with("__"))
                .collect();
            status["config_key_count"] = json!(user_keys.len());
        }

        ToolResult::ok(serde_json::to_string_pretty(&status).unwrap_or_default())
    }

    async fn config_reload(&self, _args: &Value) -> ToolResult {
        // Signal a config reload by updating the __reload_requested timestamp.
        // Components that watch this key can pick up the new config.
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("state store not configured"),
        };

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        store.set("config:__reload_requested", json!(now)).await;

        ToolResult::ok("config reload signaled")
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Resolve a potentially relative path against the workdir.
    fn resolve_path(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            self.workdir.join(p)
        }
    }
}

impl ToolResult {
    /// Successful tool result.
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    /// Error tool result.
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

#[async_trait]
impl ToolHandler for RadixToolHandler {
    async fn list_tools(&self) -> Vec<Tool> {
        vec![
            Tool {
                name: "read_file".into(),
                description: Some("Read a UTF-8 text file from disk".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({"path": {"type": "string"}})),
                    required: Some(vec!["path".into()]),
                },
            },
            Tool {
                name: "write_file".into(),
                description: Some("Write a UTF-8 text file to disk (creates parent dirs)".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "path": {"type": "string"},
                        "content": {"type": "string"}
                    })),
                    required: Some(vec!["path".into(), "content".into()]),
                },
            },
            Tool {
                name: "edit_file".into(),
                description: Some(
                    "Replace the first occurrence of old_text with new_text in a file".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "path": {"type": "string"},
                        "old_text": {"type": "string"},
                        "new_text": {"type": "string"}
                    })),
                    required: Some(vec!["path".into(), "old_text".into(), "new_text".into()]),
                },
            },
            Tool {
                name: "list_directory".into(),
                description: Some("List files in a directory".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({"path": {"type": "string"}})),
                    required: Some(vec!["path".into()]),
                },
            },
            Tool {
                name: "run_command".into(),
                description: Some(
                    "Run a shell command. Supports background, pty, timeout, workdir, env.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "command": {"type": "string", "description": "Shell command to execute"},
                        "workdir": {"type": "string", "description": "Working directory"},
                        "background": {"type": "boolean", "description": "Run in background"},
                        "pty": {"type": "boolean", "description": "Use pseudo-terminal"},
                        "timeout": {"type": "integer", "description": "Timeout in seconds"},
                        "env": {"type": "object", "description": "Additional environment variables"}
                    })),
                    required: Some(vec!["command".into()]),
                },
            },
            Tool {
                name: "process".into(),
                description: Some(
                    "Manage background shell sessions: list, poll, write, kill.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "action": {"type": "string", "enum": ["list", "poll", "write", "kill"]},
                        "sessionId": {"type": "string"},
                        "timeout": {"type": "integer"},
                        "data": {"type": "string"}
                    })),
                    required: Some(vec!["action".into()]),
                },
            },
            Tool {
                name: "memory_search".into(),
                description: Some(
                    "Search long-term memory semantically. Returns relevant stored memories."
                        .into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "query": {"type": "string", "description": "Semantic search query"},
                        "limit": {"type": "integer", "description": "Max results (default 5)"}
                    })),
                    required: Some(vec!["query".into()]),
                },
            },
            Tool {
                name: "memory_store".into(),
                description: Some(
                    "Store a fact or decision in long-term memory with optional tags.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "content": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}}
                    })),
                    required: Some(vec!["content".into()]),
                },
            },
            Tool {
                name: "web_fetch".into(),
                description: Some(
                    "Fetch a URL and return readable content (HTML auto-converted to text).".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "url": {"type": "string"},
                        "max_chars": {"type": "integer", "description": "Max chars (default 30000)"}
                    })),
                    required: Some(vec!["url".into()]),
                },
            },
            Tool {
                name: "web_search".into(),
                description: Some("Search the web via Brave Search API.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "query": {"type": "string"},
                        "count": {"type": "integer", "description": "Number of results (default 5)"}
                    })),
                    required: Some(vec!["query".into()]),
                },
            },
            Tool {
                name: "cron_list".into(),
                description: Some("List all scheduled cron tasks.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({})),
                    required: None,
                },
            },
            Tool {
                name: "cron_add".into(),
                description: Some("Add a scheduled cron task. Provide 'cron' (5-field expr) or 'interval_secs'.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "name": {"type": "string", "description": "Human-readable task name"},
                        "command": {"type": "string", "description": "Shell command to execute"},
                        "cron": {"type": "string", "description": "Cron expression (5-field: min hour dom month dow)"},
                        "interval_secs": {"type": "integer", "description": "Run every N seconds (alternative to cron)"}
                    })),
                    required: Some(vec!["name".into(), "command".into()]),
                },
            },
            Tool {
                name: "cron_remove".into(),
                description: Some("Remove a scheduled task by id.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({"id": {"type": "string"}})),
                    required: Some(vec!["id".into()]),
                },
            },
            Tool {
                name: "cron_toggle".into(),
                description: Some("Enable or disable a scheduled task.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "id": {"type": "string"},
                        "enabled": {"type": "boolean"}
                    })),
                    required: Some(vec!["id".into(), "enabled".into()]),
                },
            },
            // ── State/DB tools ────────────────────────────────────────────
            Tool {
                name: "db_get".into(),
                description: Some(
                    "Get a value from the key-value state store. Returns JSON or null.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "key": {"type": "string", "description": "The key to retrieve"}
                    })),
                    required: Some(vec!["key".into()]),
                },
            },
            Tool {
                name: "db_put".into(),
                description: Some(
                    "Store a JSON value under a key in the state store. Overwrites existing values."
                        .into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "key": {"type": "string", "description": "The key to store under"},
                        "value": {"description": "The JSON value to store (any type)"}
                    })),
                    required: Some(vec!["key".into(), "value".into()]),
                },
            },
            Tool {
                name: "db_delete".into(),
                description: Some(
                    "Delete a key from the state store (sets to null).".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "key": {"type": "string", "description": "The key to delete"}
                    })),
                    required: Some(vec!["key".into()]),
                },
            },
            // ── Config & Runtime tools ────────────────────────────────────
            Tool {
                name: "config_get".into(),
                description: Some(
                    "Get a configuration value by key. Returns JSON or null.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "key": {"type": "string", "description": "Config key (e.g. 'model', 'routing.interactive')"}
                    })),
                    required: Some(vec!["key".into()]),
                },
            },
            Tool {
                name: "config_set".into(),
                description: Some(
                    "Set a configuration value. Triggers hot-reload for listening components.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "key": {"type": "string", "description": "Config key to set"},
                        "value": {"description": "The configuration value (any JSON type)"}
                    })),
                    required: Some(vec!["key".into(), "value".into()]),
                },
            },
            Tool {
                name: "config_list".into(),
                description: Some(
                    "List all configuration keys and their current values. Dynamically scans the store. Optional prefix filter.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "prefix": {"type": "string", "description": "Optional prefix filter for keys"}
                    })),
                    required: None,
                },
            },
            Tool {
                name: "config_delete".into(),
                description: Some(
                    "Delete a configuration key. Returns the previous value if it existed.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "key": {"type": "string", "description": "Config key to delete"}
                    })),
                    required: Some(vec!["key".into()]),
                },
            },
            Tool {
                name: "config_reload".into(),
                description: Some(
                    "Signal a configuration reload. Components watching config will pick up changes.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({})),
                    required: None,
                },
            },
            Tool {
                name: "runtime_status".into(),
                description: Some(
                    "Get runtime status: version, active components, shell sessions, health.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({})),
                    required: None,
                },
            },
        ]
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> ToolResult {
        debug!(tool = name, "MCP tool call");
        match name {
            "read_file" => self.read_file(&arguments).await,
            "write_file" => self.write_file(&arguments).await,
            "edit_file" => self.edit_file(&arguments).await,
            "list_directory" => self.list_directory(&arguments).await,
            "run_command" => self.run_command(&arguments).await,
            "process" => self.process_action(&arguments).await,
            "memory_search" => self.memory_search(&arguments).await,
            "memory_store" => self.memory_store(&arguments).await,
            "web_fetch" => self.web_fetch(&arguments).await,
            "web_search" => self.web_search(&arguments).await,
            "cron_list" => self.cron_list(&arguments).await,
            "cron_add" => self.cron_add(&arguments).await,
            "cron_remove" => self.cron_remove(&arguments).await,
            "cron_toggle" => self.cron_toggle(&arguments).await,
            "db_get" => self.db_get(&arguments).await,
            "db_put" => self.db_put(&arguments).await,
            "db_delete" => self.db_delete(&arguments).await,
            "config_get" => self.config_get(&arguments).await,
            "config_set" => self.config_set(&arguments).await,
            "config_list" => self.config_list(&arguments).await,
            "config_delete" => self.config_delete(&arguments).await,
            "config_reload" => self.config_reload(&arguments).await,
            "runtime_status" => self.runtime_status(&arguments).await,
            other => {
                warn!(tool = other, "unknown tool called via MCP");
                ToolResult::error(format!("unknown tool: {other}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_handler() -> RadixToolHandler {
        let shell = Arc::new(ShellExecutor::new());
        RadixToolHandler::new(shell, PathBuf::from("/tmp"))
    }

    #[tokio::test]
    async fn list_tools_returns_all() {
        let handler = make_handler();
        let tools = handler.list_tools().await;
        assert!(tools.len() >= 10);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"run_command"));
        assert!(names.contains(&"memory_search"));
        assert!(names.contains(&"web_fetch"));
    }

    #[tokio::test]
    async fn read_file_missing_path() {
        let handler = make_handler();
        let result = handler.call_tool("read_file", json!({})).await;
        assert!(result.is_error);
        assert!(result.content.contains("path"));
    }

    #[tokio::test]
    async fn read_file_nonexistent() {
        let handler = make_handler();
        let result = handler
            .call_tool("read_file", json!({"path": "/tmp/nonexistent_radix_test_xyz"}))
            .await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn write_and_read_file() {
        let handler = make_handler();
        let test_path = "/tmp/radix_mcp_test_write.txt";

        let write_result = handler
            .call_tool(
                "write_file",
                json!({"path": test_path, "content": "hello from MCP"}),
            )
            .await;
        assert!(!write_result.is_error);

        let read_result = handler
            .call_tool("read_file", json!({"path": test_path}))
            .await;
        assert!(!read_result.is_error);
        assert_eq!(read_result.content, "hello from MCP");

        // Cleanup
        let _ = tokio::fs::remove_file(test_path).await;
    }

    #[tokio::test]
    async fn run_command_echo() {
        let handler = make_handler();
        let result = handler
            .call_tool("run_command", json!({"command": "echo hello_mcp"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("hello_mcp"));
    }

    #[tokio::test]
    async fn list_directory_tmp() {
        let handler = make_handler();
        let result = handler
            .call_tool("list_directory", json!({"path": "/tmp"}))
            .await;
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn unknown_tool_returns_error() {
        let handler = make_handler();
        let result = handler.call_tool("nonexistent_tool", json!({})).await;
        assert!(result.is_error);
        assert!(result.content.contains("unknown tool"));
    }

    #[tokio::test]
    async fn memory_search_without_memory_returns_error() {
        let handler = make_handler();
        let result = handler
            .call_tool("memory_search", json!({"query": "test"}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn web_search_without_api_key_returns_error() {
        let handler = make_handler();
        let result = handler
            .call_tool("web_search", json!({"query": "rust lang"}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn cron_list_without_scheduler_returns_error() {
        let handler = make_handler();
        let result = handler.call_tool("cron_list", json!({})).await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn cron_add_without_scheduler_returns_error() {
        let handler = make_handler();
        let result = handler
            .call_tool("cron_add", json!({"name": "test", "command": "echo hi", "interval_secs": 60}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn cron_tools_with_scheduler() {
        let shell = Arc::new(ShellExecutor::new());
        let scheduler = Arc::new(Scheduler::new());
        let handler = RadixToolHandler::new(shell, PathBuf::from("/tmp"))
            .with_scheduler(scheduler);

        // List starts empty
        let result = handler.call_tool("cron_list", json!({})).await;
        assert!(!result.is_error);
        assert!(result.content.contains("[]"));

        // Add a task
        let result = handler
            .call_tool("cron_add", json!({
                "name": "test_task",
                "command": "echo hello",
                "interval_secs": 300
            }))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("added task:"));

        // List has one task
        let result = handler.call_tool("cron_list", json!({})).await;
        assert!(!result.is_error);
        assert!(result.content.contains("test_task"));

        // Parse id from list
        let tasks: Vec<Value> = serde_json::from_str(&result.content).unwrap();
        let task_id = tasks[0]["id"].as_str().unwrap().to_string();

        // Toggle disable
        let result = handler
            .call_tool("cron_toggle", json!({"id": &task_id, "enabled": false}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("enabled=false"));

        // Remove
        let result = handler
            .call_tool("cron_remove", json!({"id": &task_id}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("removed task:"));

        // List empty again
        let result = handler.call_tool("cron_list", json!({})).await;
        assert!(!result.is_error);
        assert!(result.content.contains("[]"));
    }

    // ── State/DB tool tests ─────────────────────────────────────────────────

    fn make_handler_with_state() -> RadixToolHandler {
        let shell = Arc::new(ShellExecutor::new());
        let state = Arc::new(pares_agens_core::InMemoryStateStore::new());
        RadixToolHandler::new(shell, PathBuf::from("/tmp"))
            .with_state_store(state)
    }

    #[tokio::test]
    async fn db_get_missing_key_returns_null() {
        let handler = make_handler_with_state();
        let result = handler.call_tool("db_get", json!({"key": "nonexistent"})).await;
        assert!(!result.is_error);
        assert_eq!(result.content, "null");
    }

    #[tokio::test]
    async fn db_put_then_get_roundtrip() {
        let handler = make_handler_with_state();

        let result = handler
            .call_tool("db_put", json!({"key": "test:foo", "value": {"bar": 42}}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("stored key: test:foo"));

        let result = handler.call_tool("db_get", json!({"key": "test:foo"})).await;
        assert!(!result.is_error);
        assert!(result.content.contains("42"));
        assert!(result.content.contains("bar"));
    }

    #[tokio::test]
    async fn db_delete_sets_null() {
        let handler = make_handler_with_state();

        handler
            .call_tool("db_put", json!({"key": "del:me", "value": "hello"}))
            .await;
        let result = handler.call_tool("db_delete", json!({"key": "del:me"})).await;
        assert!(!result.is_error);
        assert!(result.content.contains("deleted key: del:me"));

        let result = handler.call_tool("db_get", json!({"key": "del:me"})).await;
        assert!(!result.is_error);
        // After delete, value is null
        assert_eq!(result.content, "null");
    }

    #[tokio::test]
    async fn db_get_without_state_store_returns_error() {
        let handler = make_handler(); // no state store attached
        let result = handler.call_tool("db_get", json!({"key": "x"})).await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn db_tools_appear_in_tool_list() {
        let handler = make_handler_with_state();
        let tools = handler.list_tools().await;
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"db_get"));
        assert!(names.contains(&"db_put"));
        assert!(names.contains(&"db_delete"));
    }

    // ── Config & Runtime tool tests ────────────────────────────────────────────

    #[tokio::test]
    async fn config_set_and_get_roundtrip() {
        let handler = make_handler_with_state();
        let result = handler
            .call_tool("config_set", json!({"key": "model", "value": "gpt-4.1"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("config set: model"));

        let result = handler
            .call_tool("config_get", json!({"key": "model"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("gpt-4.1"));
    }

    #[tokio::test]
    async fn config_get_missing_returns_null() {
        let handler = make_handler_with_state();
        let result = handler
            .call_tool("config_get", json!({"key": "nonexistent_key"}))
            .await;
        assert!(!result.is_error);
        assert_eq!(result.content, "null");
    }

    #[tokio::test]
    async fn config_list_shows_set_keys() {
        let handler = make_handler_with_state();
        handler
            .call_tool("config_set", json!({"key": "model", "value": "test-model"}))
            .await;
        handler
            .call_tool("config_set", json!({"key": "endpoint", "value": "http://localhost"}))
            .await;

        let result = handler.call_tool("config_list", json!({})).await;
        assert!(!result.is_error);
        assert!(result.content.contains("model"));
        assert!(result.content.contains("test-model"));
    }

    #[tokio::test]
    async fn config_list_with_prefix_filter() {
        let handler = make_handler_with_state();
        handler
            .call_tool("config_set", json!({"key": "routing.interactive", "value": "fast"}))
            .await;
        handler
            .call_tool("config_set", json!({"key": "model", "value": "gpt-4"}))
            .await;

        let result = handler
            .call_tool("config_list", json!({"prefix": "routing"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("routing.interactive"));
        // Should not contain "model" since it doesn't start with "routing"
        assert!(!result.content.contains("gpt-4"));
    }

    #[tokio::test]
    async fn config_reload_signals() {
        let handler = make_handler_with_state();
        let result = handler.call_tool("config_reload", json!({})).await;
        assert!(!result.is_error);
        assert!(result.content.contains("reload signaled"));
    }

    #[tokio::test]
    async fn runtime_status_returns_components() {
        let handler = make_handler_with_state();
        let result = handler.call_tool("runtime_status", json!({})).await;
        assert!(!result.is_error);
        assert!(result.content.contains("running"));
        assert!(result.content.contains("components"));
        assert!(result.content.contains("version"));
    }

    #[tokio::test]
    async fn config_get_without_state_store_returns_error() {
        let handler = make_handler(); // no state store
        let result = handler.call_tool("config_get", json!({"key": "model"})).await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn config_runtime_tools_in_tool_list() {
        let handler = make_handler_with_state();
        let tools = handler.list_tools().await;
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"config_get"));
        assert!(names.contains(&"config_set"));
        assert!(names.contains(&"config_list"));
        assert!(names.contains(&"config_delete"));
        assert!(names.contains(&"config_reload"));
        assert!(names.contains(&"runtime_status"));
    }

    #[tokio::test]
    async fn config_delete_existing_key() {
        let handler = make_handler_with_state();
        handler
            .call_tool("config_set", json!({"key": "model", "value": "gpt-4"}))
            .await;

        let result = handler
            .call_tool("config_delete", json!({"key": "model"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("deleted config: model"));
        assert!(result.content.contains("gpt-4"));

        // Verify it's gone
        let result = handler
            .call_tool("config_get", json!({"key": "model"}))
            .await;
        assert_eq!(result.content, "null");
    }

    #[tokio::test]
    async fn config_delete_missing_key() {
        let handler = make_handler_with_state();
        let result = handler
            .call_tool("config_delete", json!({"key": "nonexistent"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn config_list_dynamic_scanning() {
        let handler = make_handler_with_state();
        // Set some custom keys not in any hardcoded list
        handler
            .call_tool("config_set", json!({"key": "custom.setting", "value": "enabled"}))
            .await;
        handler
            .call_tool("config_set", json!({"key": "custom.threshold", "value": 42}))
            .await;

        let result = handler
            .call_tool("config_list", json!({"prefix": "custom"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("custom.setting"));
        assert!(result.content.contains("custom.threshold"));
        assert!(result.content.contains("enabled"));
    }
}
