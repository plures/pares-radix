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
use pares_agens_praxis::module::PraxisModule;
use pares_agens_praxis::rule::{RuleContext, RuleResult};

use crate::browser::BrowserClient;
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
    /// OpenAI API key for media tools (image gen, TTS, vision).
    openai_api_key: Option<String>,
    /// Media output directory for generated files.
    media_dir: PathBuf,
    /// Browser CDP client for browser automation tools.
    browser: Option<Arc<BrowserClient>>,
    /// Praxis modules for constraint evaluation.
    praxis_modules: Vec<Box<dyn PraxisModule + Send + Sync>>,
}

impl RadixToolHandler {
    /// Create a new handler with required dependencies.
    pub fn new(shell: Arc<ShellExecutor>, workdir: PathBuf) -> Self {
        let media_dir = workdir.join(".radix-media");
        Self {
            shell,
            memory: None,
            scheduler: None,
            state_store: None,
            workdir,
            brave_api_key: None,
            openai_api_key: None,
            media_dir,
            browser: None,
            praxis_modules: Vec::new(),
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

    /// Set the OpenAI API key for media tools.
    pub fn with_openai_api_key(mut self, key: String) -> Self {
        self.openai_api_key = Some(key);
        self
    }

    /// Attach a state store for db_get/db_put/db_delete tools.
    pub fn with_state_store(mut self, store: Arc<dyn StateStore>) -> Self {
        self.state_store = Some(store);
        self
    }

    /// Attach praxis modules for constraint evaluation.
    pub fn with_praxis_modules(mut self, modules: Vec<Box<dyn PraxisModule + Send + Sync>>) -> Self {
        self.praxis_modules = modules;
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

    // ── Heartbeat tools ─────────────────────────────────────────────────────────

    async fn heartbeat_status(&self, _args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("state store not configured"),
        };

        // Read heartbeat config from state (same keys HeartbeatRunner uses)
        let config = store.get("heartbeat/config").await;
        let daily_count = store.get("heartbeat/daily_count").await;
        let daily_date = store.get("heartbeat/daily_date").await;
        let checklist = store.get("heartbeat/checklist").await;

        let config_obj = config.unwrap_or(json!({
            "enabled": true,
            "interval_secs": 30,
            "quiet_hours_enabled": true,
            "quiet_hours_start": 23,
            "quiet_hours_end": 8,
            "max_proactive_per_day": 6
        }));

        let checklist_items = checklist
            .and_then(|v| v.as_array().map(|a| a.len()))
            .unwrap_or(0);

        let status = json!({
            "config": config_obj,
            "daily_count": daily_count.unwrap_or(json!(0)),
            "daily_date": daily_date.unwrap_or(json!(null)),
            "checklist_item_count": checklist_items
        });

        ToolResult::ok(serde_json::to_string_pretty(&status).unwrap_or_default())
    }

    async fn heartbeat_configure(&self, args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("state store not configured"),
        };

        // Load existing config or defaults
        let mut config = store
            .get("heartbeat/config")
            .await
            .and_then(|v| serde_json::from_value::<serde_json::Map<String, Value>>(v).ok())
            .unwrap_or_else(|| {
                serde_json::from_value::<serde_json::Map<String, Value>>(json!({
                    "enabled": true,
                    "interval_secs": 30,
                    "quiet_hours_enabled": true,
                    "quiet_hours_start": 23,
                    "quiet_hours_end": 8,
                    "max_proactive_per_day": 6
                }))
                .unwrap()
            });

        // Apply provided overrides
        if let Some(enabled) = args.get("enabled") {
            config.insert("enabled".into(), enabled.clone());
        }
        if let Some(interval) = args.get("interval_secs") {
            config.insert("interval_secs".into(), interval.clone());
        }
        if let Some(quiet_enabled) = args.get("quiet_hours_enabled") {
            config.insert("quiet_hours_enabled".into(), quiet_enabled.clone());
        }
        if let Some(quiet_start) = args.get("quiet_hours_start") {
            config.insert("quiet_hours_start".into(), quiet_start.clone());
        }
        if let Some(quiet_end) = args.get("quiet_hours_end") {
            config.insert("quiet_hours_end".into(), quiet_end.clone());
        }
        if let Some(max) = args.get("max_proactive_per_day") {
            config.insert("max_proactive_per_day".into(), max.clone());
        }

        let config_value = Value::Object(config);
        store.set("heartbeat/config", config_value.clone()).await;

        ToolResult::ok(format!(
            "heartbeat config updated: {}",
            serde_json::to_string_pretty(&config_value).unwrap_or_default()
        ))
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
                "web_search": if self.brave_api_key.is_some() { "active" } else { "not_configured" },
                "media_tools": if self.openai_api_key.is_some() { "active" } else { "not_configured" }
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

    async fn runtime_restart(&self, args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("state store not configured"),
        };

        let reason = args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("manual restart requested");

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Record restart request in state store for the process supervisor to pick up
        store.set("runtime:restart_requested", json!({
            "timestamp": now,
            "reason": reason
        })).await;

        // Also record in restart history
        store.set(&format!("runtime:restart_history:{now}"), json!({
            "reason": reason,
            "requested_at": now
        })).await;

        ToolResult::ok(format!("restart signaled (reason: {reason}). Process supervisor will handle the restart."))
    }

    async fn config_schema(&self, args: &Value) -> ToolResult {
        let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("");

        // Static schema registry for known config keys
        let schema = match key {
            "model" => json!({
                "key": "model",
                "type": "string",
                "description": "Primary model for agent responses (e.g., gpt-4.1, claude-opus-4)",
                "examples": ["gpt-4.1", "claude-opus-4", "gemini-2.5-pro"]
            }),
            "routing.interactive" => json!({
                "key": "routing.interactive",
                "type": "string",
                "description": "Model used for interactive/fast responses",
                "examples": ["gpt-4.1-mini", "claude-sonnet-4"]
            }),
            "routing.background" => json!({
                "key": "routing.background",
                "type": "string",
                "description": "Model used for background/batch work",
                "examples": ["gpt-4.1", "claude-opus-4"]
            }),
            "brave_api_key" => json!({
                "key": "brave_api_key",
                "type": "string",
                "description": "Brave Search API key for web_search tool",
                "sensitive": true
            }),
            "workdir" => json!({
                "key": "workdir",
                "type": "string",
                "description": "Working directory for file and shell operations"
            }),
            "" => json!({
                "keys": [
                    "model", "routing.interactive", "routing.background",
                    "brave_api_key", "workdir"
                ],
                "description": "Pass a specific key to get its full schema. These are the known config keys."
            }),
            other => json!({
                "key": other,
                "type": "unknown",
                "description": format!("No schema registered for key: {other}. Custom keys accept any JSON value.")
            }),
        };

        ToolResult::ok(serde_json::to_string_pretty(&schema).unwrap_or_default())
    }

    // ── Media tools ────────────────────────────────────────────────────────────

    async fn image_analyze(&self, args: &Value) -> ToolResult {
        let api_key = match &self.openai_api_key {
            Some(k) => k.clone(),
            None => return ToolResult::error("OpenAI API key not configured"),
        };

        let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("Describe this image in detail.");
        let image_url = args.get("image_url").and_then(|v| v.as_str());
        let image_path = args.get("image_path").and_then(|v| v.as_str());

        let image_content = if let Some(url) = image_url {
            json!({"type": "image_url", "image_url": {"url": url}})
        } else if let Some(path) = image_path {
            let resolved = self.resolve_path(path);
            match tokio::fs::read(&resolved).await {
                Ok(bytes) => {
                    use base64::Engine;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    let mime = if path.ends_with(".png") { "image/png" }
                        else if path.ends_with(".gif") { "image/gif" }
                        else if path.ends_with(".webp") { "image/webp" }
                        else { "image/jpeg" };
                    json!({"type": "image_url", "image_url": {"url": format!("data:{mime};base64,{b64}")}})
                }
                Err(e) => return ToolResult::error(format!("failed to read image: {e}")),
            }
        } else {
            return ToolResult::error("provide either image_url or image_path");
        };

        let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("gpt-4o");
        let body = json!({
            "model": model,
            "messages": [{"role": "user", "content": [{"type": "text", "text": prompt}, image_content]}],
            "max_tokens": 1024
        });

        let client = reqwest::Client::new();
        match client.post("https://api.openai.com/v1/chat/completions").bearer_auth(&api_key).json(&body).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    return ToolResult::error(format!("API error {status}: {text}"));
                }
                match resp.json::<Value>().await {
                    Ok(data) => ToolResult::ok(data["choices"][0]["message"]["content"].as_str().unwrap_or("no response").to_string()),
                    Err(e) => ToolResult::error(format!("failed to parse response: {e}")),
                }
            }
            Err(e) => ToolResult::error(format!("request failed: {e}")),
        }
    }

    async fn image_generate(&self, args: &Value) -> ToolResult {
        let api_key = match &self.openai_api_key {
            Some(k) => k.clone(),
            None => return ToolResult::error("OpenAI API key not configured"),
        };
        let prompt = match args.get("prompt").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("missing required parameter: prompt"),
        };
        let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("gpt-image-1");
        let size = args.get("size").and_then(|v| v.as_str()).unwrap_or("1024x1024");
        let quality = args.get("quality").and_then(|v| v.as_str()).unwrap_or("auto");

        let body = json!({"model": model, "prompt": prompt, "n": 1, "size": size, "quality": quality});
        let client = reqwest::Client::new();
        match client.post("https://api.openai.com/v1/images/generations").bearer_auth(&api_key).json(&body).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    return ToolResult::error(format!("API error {status}: {text}"));
                }
                match resp.json::<Value>().await {
                    Ok(data) => {
                        if let Some(b64) = data["data"][0]["b64_json"].as_str() {
                            let _ = tokio::fs::create_dir_all(&self.media_dir).await;
                            let filename = format!("{}.png", uuid::Uuid::new_v4());
                            let filepath = self.media_dir.join(&filename);
                            use base64::Engine;
                            match base64::engine::general_purpose::STANDARD.decode(b64) {
                                Ok(bytes) => {
                                    if let Err(e) = tokio::fs::write(&filepath, &bytes).await {
                                        return ToolResult::error(format!("failed to save image: {e}"));
                                    }
                                    ToolResult::ok(json!({"path": filepath.display().to_string(), "size_bytes": bytes.len()}).to_string())
                                }
                                Err(e) => ToolResult::error(format!("failed to decode base64: {e}")),
                            }
                        } else if let Some(url) = data["data"][0]["url"].as_str() {
                            ToolResult::ok(json!({"url": url}).to_string())
                        } else {
                            ToolResult::error(format!("unexpected response format: {data}"))
                        }
                    }
                    Err(e) => ToolResult::error(format!("failed to parse response: {e}")),
                }
            }
            Err(e) => ToolResult::error(format!("request failed: {e}")),
        }
    }

    async fn tts_generate(&self, args: &Value) -> ToolResult {
        let api_key = match &self.openai_api_key {
            Some(k) => k.clone(),
            None => return ToolResult::error("OpenAI API key not configured"),
        };
        let text = match args.get("text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("missing required parameter: text"),
        };
        let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("gpt-4o-mini-tts");
        let voice = args.get("voice").and_then(|v| v.as_str()).unwrap_or("alloy");

        let body = json!({"model": model, "input": text, "voice": voice});
        let client = reqwest::Client::new();
        match client.post("https://api.openai.com/v1/audio/speech").bearer_auth(&api_key).json(&body).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let err_text = resp.text().await.unwrap_or_default();
                    return ToolResult::error(format!("API error {status}: {err_text}"));
                }
                match resp.bytes().await {
                    Ok(bytes) => {
                        let _ = tokio::fs::create_dir_all(&self.media_dir).await;
                        let filename = format!("{}.mp3", uuid::Uuid::new_v4());
                        let filepath = self.media_dir.join(&filename);
                        if let Err(e) = tokio::fs::write(&filepath, &bytes).await {
                            return ToolResult::error(format!("failed to save audio: {e}"));
                        }
                        ToolResult::ok(json!({"path": filepath.display().to_string(), "size_bytes": bytes.len()}).to_string())
                    }
                    Err(e) => ToolResult::error(format!("failed to read response: {e}")),
                }
            }
            Err(e) => ToolResult::error(format!("request failed: {e}")),
        }
    }

    async fn pdf_analyze(&self, args: &Value) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => self.resolve_path(p),
            None => return ToolResult::error("missing required parameter: path"),
        };
        if !path.exists() {
            return ToolResult::error(format!("file not found: {}", path.display()));
        }

        let output = tokio::process::Command::new("pdftotext").arg(path.to_string_lossy().as_ref()).arg("-").output().await;
        let text = match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
            Ok(out) => return ToolResult::error(format!("pdftotext failed: {}", String::from_utf8_lossy(&out.stderr))),
            Err(e) => return ToolResult::error(format!("pdftotext not found or failed: {e}. Install poppler-utils.")),
        };

        let prompt = match args.get("prompt").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                let truncated = if text.len() > 50_000 { format!("{}\n\n[truncated]", &text[..50_000]) } else { text };
                return ToolResult::ok(truncated);
            }
        };

        let api_key = match &self.openai_api_key {
            Some(k) => k.clone(),
            None => return ToolResult::ok(format!("[no API key for analysis]\n\n{}", if text.len() > 50_000 { &text[..50_000] } else { &text })),
        };

        let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("gpt-4o-mini");
        let max_text = 100_000;
        let pdf_text = if text.len() > max_text { &text[..max_text] } else { &text };
        let body = json!({
            "model": model,
            "messages": [
                {"role": "system", "content": "You are analyzing a PDF document."},
                {"role": "user", "content": format!("{prompt}\n\n---\n\n{pdf_text}")}
            ],
            "max_tokens": 2048
        });

        let client = reqwest::Client::new();
        match client.post("https://api.openai.com/v1/chat/completions").bearer_auth(&api_key).json(&body).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let err_text = resp.text().await.unwrap_or_default();
                    return ToolResult::error(format!("API error {status}: {err_text}"));
                }
                match resp.json::<Value>().await {
                    Ok(data) => ToolResult::ok(data["choices"][0]["message"]["content"].as_str().unwrap_or("no response").to_string()),
                    Err(e) => ToolResult::error(format!("failed to parse response: {e}")),
                }
            }
            Err(e) => ToolResult::error(format!("request failed: {e}")),
        }
    }

    async fn video_generate(&self, _args: &Value) -> ToolResult {
        ToolResult::error("video generation provider not configured. Configure a provider via config_set to enable.")
    }

    async fn music_generate(&self, _args: &Value) -> ToolResult {
        ToolResult::error("music generation provider not configured. Configure a provider via config_set to enable.")
    }

    // ── Remote Node tools ──────────────────────────────────────────────────────

    /// Simple shell quoting: wrap in single quotes, escaping existing single quotes.
    fn shell_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''" ))
    }

    /// Resolve a node identifier (name/id/IP) to an SSH target.
    /// If the identifier looks like an IP or hostname, use it directly.
    /// Otherwise, look up in state store under "nodes:<name>".
    async fn resolve_node(&self, node_id: &str) -> Result<String, String> {
        // Direct IP/hostname detection
        if node_id.contains('.') || node_id.contains(':') {
            return Ok(node_id.to_string());
        }
        // Look up from state store
        let store = self.state_store.as_ref()
            .ok_or_else(|| "state store not configured".to_string())?;
        let key = format!("nodes:{node_id}");
        match store.get(&key).await {
            Some(val) => {
                // Expect {"host": "...", ...}
                if let Some(host) = val.get("host").and_then(|h| h.as_str()) {
                    Ok(host.to_string())
                } else {
                    Err(format!("node '{node_id}' has no 'host' field"))
                }
            }
            None => Err(format!("node '{node_id}' not found")),
        }
    }

    async fn node_file_read(&self, args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("node operations require state store (not configured)"),
        };
        let node_id = match args.get("node").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::error("missing required parameter: node"),
        };
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("missing required parameter: path"),
        };
        let host = match self.resolve_node(node_id).await {
            Ok(h) => h,
            Err(e) => return ToolResult::error(e),
        };
        // Use SSH to read the file
        let cmd = format!("ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 {} cat {}",
            Self::shell_quote(&host),
            Self::shell_quote(path));
        use pares_agens_core::shell_executor::ExecRequest;
        let req = ExecRequest {
            command: cmd,
            workdir: None,
            background: false,
            pty: false,
            timeout_secs: Some(30),
            env: std::collections::HashMap::new(),
            yield_ms: None,
        };
        {
            let output = self.shell.exec(req).await;
            if output.exit_code == Some(0) {
                ToolResult::ok(output.stdout)
            } else {
                ToolResult::error(format!("SSH read failed: {}", output.stderr))
            }
        }
    }

    async fn node_file_write(&self, args: &Value) -> ToolResult {
        let _store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("node operations require state store (not configured)"),
        };
        let node_id = match args.get("node").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::error("missing required parameter: node"),
        };
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("missing required parameter: path"),
        };
        let content = match args.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("missing required parameter: content"),
        };
        let host = match self.resolve_node(node_id).await {
            Ok(h) => h,
            Err(e) => return ToolResult::error(e),
        };
        // Pipe content via SSH
        let cmd = format!("ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 {} 'cat > {}'",
            Self::shell_quote(&host),
            Self::shell_quote(path));
        use pares_agens_core::shell_executor::ExecRequest;
        let req = ExecRequest {
            command: format!("echo {} | {}", Self::shell_quote(content), cmd),
            workdir: None,
            background: false,
            pty: false,
            timeout_secs: Some(30),
            env: std::collections::HashMap::new(),
            yield_ms: None,
        };
        {
            let output = self.shell.exec(req).await;
            if output.exit_code == Some(0) {
                ToolResult::ok(format!("wrote {} bytes to {path} on node", content.len()))
            } else {
                ToolResult::error(format!("SSH write failed: {}", output.stderr))
            }
        }
    }

    async fn node_dir_list(&self, args: &Value) -> ToolResult {
        let _store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("node operations require state store (not configured)"),
        };
        let node_id = match args.get("node").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::error("missing required parameter: node"),
        };
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("missing required parameter: path"),
        };
        let host = match self.resolve_node(node_id).await {
            Ok(h) => h,
            Err(e) => return ToolResult::error(e),
        };
        let cmd = format!("ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 {} ls -la {}",
            Self::shell_quote(&host),
            Self::shell_quote(path));
        use pares_agens_core::shell_executor::ExecRequest;
        let req = ExecRequest {
            command: cmd,
            workdir: None,
            background: false,
            pty: false,
            timeout_secs: Some(30),
            env: std::collections::HashMap::new(),
            yield_ms: None,
        };
        {
            let output = self.shell.exec(req).await;
            if output.exit_code == Some(0) {
                ToolResult::ok(output.stdout)
            } else {
                ToolResult::error(format!("SSH dir list failed: {}", output.stderr))
            }
        }
    }

    async fn node_dir_fetch(&self, args: &Value) -> ToolResult {
        let _store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("node operations require state store (not configured)"),
        };
        let node_id = match args.get("node").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::error("missing required parameter: node"),
        };
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("missing required parameter: path"),
        };
        let local_path = args.get("local_path").and_then(|v| v.as_str())
            .unwrap_or("/tmp/node-fetch");
        let host = match self.resolve_node(node_id).await {
            Ok(h) => h,
            Err(e) => return ToolResult::error(e),
        };
        // tar + ssh to fetch directory
        let cmd = format!(
            "mkdir -p {} && ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 {} 'tar czf - -C {} .' | tar xzf - -C {}",
            Self::shell_quote(local_path),
            Self::shell_quote(&host),
            Self::shell_quote(path),
            Self::shell_quote(local_path),
        );
        use pares_agens_core::shell_executor::ExecRequest;
        let req = ExecRequest {
            command: cmd,
            workdir: None,
            background: false,
            pty: false,
            timeout_secs: Some(60),
            env: std::collections::HashMap::new(),
            yield_ms: None,
        };
        {
            let output = self.shell.exec(req).await;
            if output.exit_code == Some(0) {
                ToolResult::ok(format!("fetched {path} to {local_path}"))
            } else {
                ToolResult::error(format!("SSH dir fetch failed: {}", output.stderr))
            }
        }
    }

    async fn node_status(&self, _args: &Value) -> ToolResult {
        let store = match &self.state_store {
            Some(s) => s,
            None => return ToolResult::error("node operations require state store (not configured)"),
        };
        // List all keys starting with "nodes:"
        let keys = store.keys_with_prefix("nodes:").await;
        if keys.is_empty() {
            return ToolResult::ok("no nodes configured".to_string());
        }
        let mut status_lines = Vec::new();
        for key in &keys {
            if let Some(val) = store.get(key).await {
                let name = key.strip_prefix("nodes:").unwrap_or(key);
                let host = val.get("host").and_then(|h| h.as_str()).unwrap_or("unknown");
                status_lines.push(format!("• {name}: {host}"));
            }
        }
        ToolResult::ok(format!("Configured nodes ({}):\n{}", keys.len(), status_lines.join("\n")))
    }

    // ── Browser tools ─────────────────────────────────────────────────────────

    async fn browser_status(&self, _args: &Value) -> ToolResult {
        let browser = match &self.browser {
            Some(b) => b,
            None => return ToolResult::error("browser not configured. Set CDP endpoint via config."),
        };
        if !browser.is_available().await {
            return ToolResult::ok(json!({"available": false, "message": "no browser reachable at CDP endpoint"}).to_string());
        }
        match browser.version().await {
            Ok(version) => ToolResult::ok(json!({"available": true, "version": version}).to_string()),
            Err(e) => ToolResult::ok(json!({"available": false, "error": e}).to_string()),
        }
    }

    async fn browser_navigate(&self, args: &Value) -> ToolResult {
        let browser = match &self.browser {
            Some(b) => b,
            None => return ToolResult::error("browser not configured"),
        };
        let url = match args.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return ToolResult::error("missing required parameter: url"),
        };
        match browser.navigate(url).await {
            Ok(result) => ToolResult::ok(format!("navigated to {url}: {result}")),
            Err(e) => ToolResult::error(format!("navigation failed: {e}")),
        }
    }

    async fn browser_snapshot(&self, _args: &Value) -> ToolResult {
        let browser = match &self.browser {
            Some(b) => b,
            None => return ToolResult::error("browser not configured"),
        };
        match browser.snapshot().await {
            Ok(text) => ToolResult::ok(text),
            Err(e) => ToolResult::error(format!("snapshot failed: {e}")),
        }
    }

    async fn browser_screenshot(&self, args: &Value) -> ToolResult {
        let browser = match &self.browser {
            Some(b) => b,
            None => return ToolResult::error("browser not configured"),
        };
        let format = args.get("format").and_then(|v| v.as_str());
        match browser.screenshot(format).await {
            Ok(base64_data) => {
                let ext = format.unwrap_or("png");
                let filename = format!("screenshot-{}.{ext}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
                let path = self.media_dir.join(&filename);
                if let Err(e) = tokio::fs::create_dir_all(&self.media_dir).await {
                    return ToolResult::error(format!("failed to create media dir: {e}"));
                }
                match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &base64_data) {
                    Ok(bytes) => {
                        if let Err(e) = tokio::fs::write(&path, &bytes).await {
                            return ToolResult::error(format!("failed to save screenshot: {e}"));
                        }
                        ToolResult::ok(format!("screenshot saved: {} ({} bytes)", path.display(), bytes.len()))
                    }
                    Err(e) => ToolResult::error(format!("failed to decode screenshot: {e}")),
                }
            }
            Err(e) => ToolResult::error(format!("screenshot failed: {e}")),
        }
    }

    async fn browser_click(&self, args: &Value) -> ToolResult {
        let browser = match &self.browser {
            Some(b) => b,
            None => return ToolResult::error("browser not configured"),
        };
        let selector = args.get("selector").and_then(|v| v.as_str());
        let x = args.get("x").and_then(|v| v.as_f64());
        let y = args.get("y").and_then(|v| v.as_f64());
        match browser.click(selector, x, y).await {
            Ok(msg) => ToolResult::ok(msg),
            Err(e) => ToolResult::error(format!("click failed: {e}")),
        }
    }

    async fn browser_type(&self, args: &Value) -> ToolResult {
        let browser = match &self.browser {
            Some(b) => b,
            None => return ToolResult::error("browser not configured"),
        };
        let text = match args.get("text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("missing required parameter: text"),
        };
        let selector = args.get("selector").and_then(|v| v.as_str());
        match browser.type_text(text, selector).await {
            Ok(msg) => ToolResult::ok(msg),
            Err(e) => ToolResult::error(format!("type failed: {e}")),
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Resolve a potentially relative path against the workdir.
    // ── Praxis tools ──────────────────────────────────────────────────────────

    async fn praxis_evaluate(&self, args: &Value) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a.to_string(),
            None => return ToolResult::error("missing required parameter: action"),
        };

        let payload = args.get("payload").cloned().unwrap_or(json!({}));
        let module_filter = args.get("module").and_then(|v| v.as_str());

        if self.praxis_modules.is_empty() {
            return ToolResult::ok(json!({
                "warning": "no praxis modules loaded",
                "results": []
            }).to_string());
        }

        let ctx = RuleContext::new(&action, payload);
        let mut all_results: Vec<Value> = Vec::new();

        for module in &self.praxis_modules {
            if let Some(filter) = module_filter {
                if module.name() != filter {
                    continue;
                }
            }

            let results = module.evaluate_all(&ctx);
            for (rule_name, result) in results {
                let (status, message) = match &result {
                    RuleResult::Pass => ("pass", None),
                    RuleResult::Fail { reason } => ("fail", Some(reason.clone())),
                    RuleResult::Warning { message } => ("warning", Some(message.clone())),
                    RuleResult::Gate { action: _, rationale } => ("gate", Some(rationale.clone())),
                };

                let mut entry = json!({
                    "module": module.name(),
                    "rule": rule_name,
                    "status": status,
                });
                if let Some(msg) = message {
                    entry["message"] = Value::String(msg);
                }
                all_results.push(entry);
            }
        }

        let failures = all_results.iter()
            .filter(|r| r["status"] == "fail" || r["status"] == "gate")
            .count();
        let warnings = all_results.iter()
            .filter(|r| r["status"] == "warning")
            .count();

        ToolResult::ok(json!({
            "action": action,
            "total_rules": all_results.len(),
            "failures": failures,
            "warnings": warnings,
            "passed": all_results.len() - failures - warnings,
            "results": all_results
        }).to_string())
    }

    async fn praxis_list(&self, args: &Value) -> ToolResult {
        let module_filter = args.get("module").and_then(|v| v.as_str());

        if self.praxis_modules.is_empty() {
            return ToolResult::ok(json!({
                "modules": [],
                "total_rules": 0,
                "note": "no praxis modules loaded — use with_praxis_modules() to configure"
            }).to_string());
        }

        let mut modules_info: Vec<Value> = Vec::new();
        let mut total_rules = 0;

        for module in &self.praxis_modules {
            if let Some(filter) = module_filter {
                if module.name() != filter {
                    continue;
                }
            }

            let rules = module.rules();
            let audit = module.audit();
            total_rules += rules.len();

            let rule_list: Vec<Value> = rules.iter().map(|r| {
                json!({
                    "name": r.name(),
                    "category": format!("{:?}", r.category()),
                })
            }).collect();

            modules_info.push(json!({
                "name": module.name(),
                "rule_count": rules.len(),
                "completeness_pct": audit.completeness_pct,
                "expectations": module.expectations(),
                "rules": rule_list,
            }));
        }

        ToolResult::ok(json!({
            "modules": modules_info,
            "total_rules": total_rules
        }).to_string())
    }

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
        let mut tools = vec![
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
            // ── Heartbeat tools ───────────────────────────────────────────
            Tool {
                name: "heartbeat_status".into(),
                description: Some(
                    "Get heartbeat system status: config, daily count, checklist items.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({})),
                    required: None,
                },
            },
            Tool {
                name: "heartbeat_configure".into(),
                description: Some(
                    "Update heartbeat configuration. Provide only the fields to change.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "enabled": {"type": "boolean", "description": "Enable/disable heartbeat"},
                        "interval_secs": {"type": "integer", "description": "Tick interval in seconds"},
                        "quiet_hours_enabled": {"type": "boolean", "description": "Enable quiet hours"},
                        "quiet_hours_start": {"type": "integer", "description": "Quiet start hour (0-23)"},
                        "quiet_hours_end": {"type": "integer", "description": "Quiet end hour (0-23)"},
                        "max_proactive_per_day": {"type": "integer", "description": "Max proactive messages/day"}
                    })),
                    required: None,
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
            Tool {
                name: "runtime_restart".into(),
                description: Some(
                    "Signal a graceful restart. The process supervisor handles the actual restart.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "reason": {"type": "string", "description": "Reason for restart"}
                    })),
                    required: None,
                },
            },
            Tool {
                name: "config_schema".into(),
                description: Some(
                    "Look up the schema for a config key. Pass key='' to list all known keys.".into(),
                ),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "key": {"type": "string", "description": "Config key to look up schema for (empty for full list)"}
                    })),
                    required: None,
                },
            },
            // ── Browser automation tools ──────────────────────────────
            Tool {
                name: "browser_status".into(),
                description: Some("Check if a browser is available via CDP.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({})),
                    required: None,
                },
            },
            Tool {
                name: "browser_navigate".into(),
                description: Some("Navigate the browser to a URL.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "url": {"type": "string", "description": "URL to navigate to"}
                    })),
                    required: Some(vec!["url".into()]),
                },
            },
            Tool {
                name: "browser_snapshot".into(),
                description: Some("Get page content as accessibility tree or text extract.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({})),
                    required: None,
                },
            },
            Tool {
                name: "browser_screenshot".into(),
                description: Some("Capture a screenshot of the current page.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "format": {"type": "string", "enum": ["png", "jpeg"], "description": "Image format (default png)"}
                    })),
                    required: None,
                },
            },
            Tool {
                name: "browser_click".into(),
                description: Some("Click an element by CSS selector or x/y coordinates.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "selector": {"type": "string", "description": "CSS selector of element to click"},
                        "x": {"type": "number", "description": "X coordinate"},
                        "y": {"type": "number", "description": "Y coordinate"}
                    })),
                    required: None,
                },
            },
            Tool {
                name: "browser_type".into(),
                description: Some("Type text into the focused element or a specified selector.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "text": {"type": "string", "description": "Text to type"},
                        "selector": {"type": "string", "description": "Optional CSS selector to focus first"}
                    })),
                    required: Some(vec!["text".into()]),
                },
            },
            // ── Media tools ───────────────────────────────────────────────
            Tool {
                name: "image_analyze".into(),
                description: Some("Analyze an image using a vision model (GPT-4o). Accepts image_url or image_path.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "image_url": {"type": "string", "description": "URL of the image to analyze"},
                        "image_path": {"type": "string", "description": "Local file path of the image"},
                        "prompt": {"type": "string", "description": "Analysis prompt (default: describe the image)"},
                        "model": {"type": "string", "description": "Vision model to use (default: gpt-4o)"}
                    })),
                    required: None,
                },
            },
            Tool {
                name: "image_generate".into(),
                description: Some("Generate an image from a text prompt via OpenAI (DALL-E / gpt-image-1).".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "prompt": {"type": "string", "description": "Image generation prompt"},
                        "model": {"type": "string", "description": "Model (default: gpt-image-1)"},
                        "size": {"type": "string", "description": "Image size (default: 1024x1024)"},
                        "quality": {"type": "string", "description": "Quality: auto, low, medium, high"}
                    })),
                    required: Some(vec!["prompt".into()]),
                },
            },
            Tool {
                name: "tts_generate".into(),
                description: Some("Generate speech audio from text via OpenAI TTS. Returns path to MP3 file.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "text": {"type": "string", "description": "Text to convert to speech"},
                        "voice": {"type": "string", "description": "Voice: alloy, echo, fable, onyx, nova, shimmer"},
                        "model": {"type": "string", "description": "TTS model (default: gpt-4o-mini-tts)"}
                    })),
                    required: Some(vec!["text".into()]),
                },
            },
            Tool {
                name: "pdf_analyze".into(),
                description: Some("Extract text from a PDF and optionally analyze with a model. Requires pdftotext.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "path": {"type": "string", "description": "Path to PDF file"},
                        "prompt": {"type": "string", "description": "Analysis prompt (omit to just extract text)"},
                        "model": {"type": "string", "description": "Model for analysis (default: gpt-4o-mini)"}
                    })),
                    required: Some(vec!["path".into()]),
                },
            },
            Tool {
                name: "video_generate".into(),
                description: Some("Generate a video from a prompt. Requires an external provider.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({"prompt": {"type": "string", "description": "Video generation prompt"}})),
                    required: Some(vec!["prompt".into()]),
                },
            },
            Tool {
                name: "music_generate".into(),
                description: Some("Generate music from a prompt. Requires an external provider.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({"prompt": {"type": "string", "description": "Music generation prompt"}})),
                    required: Some(vec!["prompt".into()]),
                },
            },
        ];

        // ── Remote Node tools (only available when state store is configured) ──
        if self.state_store.is_some() {
            tools.push(Tool {
                name: "node_file_read".into(),
                description: Some("Read a file from a remote node via SSH.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "node": {"type": "string", "description": "Node name, id, or IP address"},
                        "path": {"type": "string", "description": "Absolute path to the file on the node"}
                    })),
                    required: Some(vec!["node".into(), "path".into()]),
                },
            });
            tools.push(Tool {
                name: "node_file_write".into(),
                description: Some("Write a file to a remote node via SSH.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "node": {"type": "string", "description": "Node name, id, or IP address"},
                        "path": {"type": "string", "description": "Absolute path on the node"},
                        "content": {"type": "string", "description": "File content to write"}
                    })),
                    required: Some(vec!["node".into(), "path".into(), "content".into()]),
                },
            });
            tools.push(Tool {
                name: "node_dir_list".into(),
                description: Some("List directory contents on a remote node via SSH.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "node": {"type": "string", "description": "Node name, id, or IP address"},
                        "path": {"type": "string", "description": "Absolute path to directory on the node"}
                    })),
                    required: Some(vec!["node".into(), "path".into()]),
                },
            });
            tools.push(Tool {
                name: "node_dir_fetch".into(),
                description: Some("Fetch a directory tree from a remote node as a tarball via SSH.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "node": {"type": "string", "description": "Node name, id, or IP address"},
                        "path": {"type": "string", "description": "Absolute path to directory on the node"},
                        "local_path": {"type": "string", "description": "Local path to save the fetched tree"}
                    })),
                    required: Some(vec!["node".into(), "path".into()]),
                },
            });
            tools.push(Tool {
                name: "node_status".into(),
                description: Some("Show status of configured remote nodes.".into()),
                input_schema: ToolInputSchema {
                    schema_type: "object".into(),
                    properties: Some(json!({
                        "node": {"type": "string", "description": "Optional: specific node to check (omit for all)"}
                    })),
                    required: None,
                },
            });
        }

        // ── Praxis tools (always available) ───────────────────────────────────
        tools.push(Tool {
            name: "praxis_evaluate".into(),
            description: Some(
                "Evaluate praxis rules/constraints against a context. Returns pass/fail/warning results.".into(),
            ),
            input_schema: ToolInputSchema {
                schema_type: "object".into(),
                properties: Some(json!({
                    "action": {"type": "string", "description": "The action being evaluated (e.g. 'send_email', 'deploy')"},
                    "payload": {"type": "object", "description": "Context payload for rule evaluation"},
                    "module": {"type": "string", "description": "Optional: specific module to evaluate (safety, agent_lifecycle, task_routing, coordination). Omit for all."}
                })),
                required: Some(vec!["action".into()]),
            },
        });
        tools.push(Tool {
            name: "praxis_list".into(),
            description: Some(
                "List loaded praxis modules, their rules, and completeness audit.".into(),
            ),
            input_schema: ToolInputSchema {
                schema_type: "object".into(),
                properties: Some(json!({
                    "module": {"type": "string", "description": "Optional: specific module to inspect (omit for all)"}
                })),
                required: None,
            },
        });

        tools
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
            "heartbeat_status" => self.heartbeat_status(&arguments).await,
            "heartbeat_configure" => self.heartbeat_configure(&arguments).await,
            "db_get" => self.db_get(&arguments).await,
            "db_put" => self.db_put(&arguments).await,
            "db_delete" => self.db_delete(&arguments).await,
            "config_get" => self.config_get(&arguments).await,
            "config_set" => self.config_set(&arguments).await,
            "config_list" => self.config_list(&arguments).await,
            "config_delete" => self.config_delete(&arguments).await,
            "config_reload" => self.config_reload(&arguments).await,
            "runtime_status" => self.runtime_status(&arguments).await,
            "runtime_restart" => self.runtime_restart(&arguments).await,
            "config_schema" => self.config_schema(&arguments).await,
            "browser_status" => self.browser_status(&arguments).await,
            "browser_navigate" => self.browser_navigate(&arguments).await,
            "browser_snapshot" => self.browser_snapshot(&arguments).await,
            "browser_screenshot" => self.browser_screenshot(&arguments).await,
            "browser_click" => self.browser_click(&arguments).await,
            "browser_type" => self.browser_type(&arguments).await,
            "image_analyze" => self.image_analyze(&arguments).await,
            "image_generate" => self.image_generate(&arguments).await,
            "tts_generate" => self.tts_generate(&arguments).await,
            "pdf_analyze" => self.pdf_analyze(&arguments).await,
            "video_generate" => self.video_generate(&arguments).await,
            "music_generate" => self.music_generate(&arguments).await,
            "node_file_read" => self.node_file_read(&arguments).await,
            "node_file_write" => self.node_file_write(&arguments).await,
            "node_dir_list" => self.node_dir_list(&arguments).await,
            "node_dir_fetch" => self.node_dir_fetch(&arguments).await,
            "node_status" => self.node_status(&arguments).await,
            "praxis_evaluate" => self.praxis_evaluate(&arguments).await,
            "praxis_list" => self.praxis_list(&arguments).await,
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

    #[tokio::test]
    async fn heartbeat_status_returns_defaults() {
        let handler = make_handler_with_state();
        let result = handler.call_tool("heartbeat_status", json!({})).await;
        assert!(!result.is_error);
        assert!(result.content.contains("enabled"));
        assert!(result.content.contains("interval_secs"));
        assert!(result.content.contains("quiet_hours_start"));
    }

    #[tokio::test]
    async fn heartbeat_configure_updates_config() {
        let handler = make_handler_with_state();
        let result = handler
            .call_tool("heartbeat_configure", json!({"enabled": false, "interval_secs": 60}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("false"));
        assert!(result.content.contains("60"));

        // Verify it persisted
        let status = handler.call_tool("heartbeat_status", json!({})).await;
        assert!(status.content.contains("false"));
        assert!(status.content.contains("60"));
    }

    #[tokio::test]
    async fn heartbeat_configure_partial_update() {
        let handler = make_handler_with_state();
        // Only change one field
        handler
            .call_tool("heartbeat_configure", json!({"max_proactive_per_day": 10}))
            .await;
        let result = handler.call_tool("heartbeat_status", json!({})).await;
        assert!(!result.is_error);
        // Default enabled should still be true
        assert!(result.content.contains("true"));
        // But max should be updated
        assert!(result.content.contains("10"));
    }

    #[tokio::test]
    async fn heartbeat_status_without_state_store_errors() {
        let handler = make_handler();
        let result = handler.call_tool("heartbeat_status", json!({})).await;
        assert!(result.is_error);
        assert!(result.content.contains("state store not configured"));
    }

    #[tokio::test]
    async fn heartbeat_tools_in_tool_list() {
        let handler = make_handler();
        let tools = handler.list_tools().await;
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"heartbeat_status"));
        assert!(names.contains(&"heartbeat_configure"));
    }

    #[tokio::test]
    async fn runtime_restart_signals() {
        let handler = make_handler_with_state();
        let result = handler
            .call_tool("runtime_restart", json!({"reason": "config change"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("restart signaled"));
        assert!(result.content.contains("config change"));
    }

    #[tokio::test]
    async fn runtime_restart_default_reason() {
        let handler = make_handler_with_state();
        let result = handler
            .call_tool("runtime_restart", json!({}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("manual restart requested"));
    }

    #[tokio::test]
    async fn config_schema_known_key() {
        let handler = make_handler();
        let result = handler
            .call_tool("config_schema", json!({"key": "model"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("model"));
        assert!(result.content.contains("string"));
    }

    #[tokio::test]
    async fn config_schema_list_all() {
        let handler = make_handler();
        let result = handler
            .call_tool("config_schema", json!({"key": ""}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("keys"));
        assert!(result.content.contains("model"));
    }

    #[tokio::test]
    async fn config_schema_unknown_key() {
        let handler = make_handler();
        let result = handler
            .call_tool("config_schema", json!({"key": "unknown.thing"}))
            .await;
        assert!(!result.is_error);
        assert!(result.content.contains("unknown"));
    }

    #[tokio::test]
    async fn new_tools_in_tool_list() {
        let handler = make_handler();
        let tools = handler.list_tools().await;
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"runtime_restart"));
        assert!(names.contains(&"config_schema"));
    }

    // ── Remote Node tool tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn node_tools_in_tool_list() {
        let handler = make_handler_with_state();
        let tools = handler.list_tools().await;
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"node_file_read"));
        assert!(names.contains(&"node_file_write"));
        assert!(names.contains(&"node_dir_list"));
        assert!(names.contains(&"node_dir_fetch"));
        assert!(names.contains(&"node_status"));
    }

    #[tokio::test]
    async fn node_file_read_missing_params() {
        let handler = make_handler_with_state();
        let result = handler.call_tool("node_file_read", json!({})).await;
        assert!(result.is_error);
        assert!(result.content.contains("node"));

        let result = handler.call_tool("node_file_read", json!({"node": "test"})).await;
        assert!(result.is_error);
        assert!(result.content.contains("path"));
    }

    #[tokio::test]
    async fn node_file_read_unknown_node() {
        let handler = make_handler_with_state();
        let result = handler
            .call_tool("node_file_read", json!({"node": "nonexistent", "path": "/etc/hostname"}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn node_status_empty_returns_info() {
        let handler = make_handler_with_state();
        let result = handler.call_tool("node_status", json!({})).await;
        assert!(!result.is_error);
        assert!(result.content.contains("no nodes configured"));
    }

    #[tokio::test]
    async fn node_status_without_state_store_errors() {
        let handler = make_handler();
        let result = handler.call_tool("node_status", json!({})).await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn node_resolve_direct_host() {
        let handler = make_handler_with_state();
        // Direct host format should resolve without state store lookup
        let result = handler
            .call_tool("node_file_read", json!({"node": "192.168.1.1", "path": "/etc/hostname"}))
            .await;
        // Will fail SSH connection but should NOT fail on "node not found"
        assert!(result.is_error);
        assert!(!result.content.contains("not found"));
    }

    // ── Media tool tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn media_tools_in_tool_list() {
        let handler = make_handler();
        let tools = handler.list_tools().await;
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"image_analyze"));
        assert!(names.contains(&"image_generate"));
        assert!(names.contains(&"tts_generate"));
        assert!(names.contains(&"pdf_analyze"));
        assert!(names.contains(&"video_generate"));
        assert!(names.contains(&"music_generate"));
    }

    #[tokio::test]
    async fn image_analyze_without_api_key_returns_error() {
        let handler = make_handler();
        let result = handler
            .call_tool("image_analyze", json!({"image_url": "https://example.com/img.jpg"}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn image_analyze_missing_image_returns_error() {
        let shell = Arc::new(ShellExecutor::new());
        let handler = RadixToolHandler::new(shell, PathBuf::from("/tmp"))
            .with_openai_api_key("sk-test".into());
        let result = handler
            .call_tool("image_analyze", json!({"prompt": "describe"}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("image_url or image_path"));
    }

    #[tokio::test]
    async fn image_generate_without_api_key_returns_error() {
        let handler = make_handler();
        let result = handler
            .call_tool("image_generate", json!({"prompt": "a cat"}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn image_generate_missing_prompt_returns_error() {
        let shell = Arc::new(ShellExecutor::new());
        let handler = RadixToolHandler::new(shell, PathBuf::from("/tmp"))
            .with_openai_api_key("sk-test".into());
        let result = handler
            .call_tool("image_generate", json!({}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("prompt"));
    }

    #[tokio::test]
    async fn tts_generate_without_api_key_returns_error() {
        let handler = make_handler();
        let result = handler
            .call_tool("tts_generate", json!({"text": "hello"}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn tts_generate_missing_text_returns_error() {
        let shell = Arc::new(ShellExecutor::new());
        let handler = RadixToolHandler::new(shell, PathBuf::from("/tmp"))
            .with_openai_api_key("sk-test".into());
        let result = handler
            .call_tool("tts_generate", json!({}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("text"));
    }

    #[tokio::test]
    async fn pdf_analyze_missing_path_returns_error() {
        let handler = make_handler();
        let result = handler
            .call_tool("pdf_analyze", json!({}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("path"));
    }

    #[tokio::test]
    async fn pdf_analyze_nonexistent_file_returns_error() {
        let handler = make_handler();
        let result = handler
            .call_tool("pdf_analyze", json!({"path": "/tmp/nonexistent_radix_test.pdf"}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn video_generate_returns_not_configured() {
        let handler = make_handler();
        let result = handler
            .call_tool("video_generate", json!({"prompt": "a sunset"}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn music_generate_returns_not_configured() {
        let handler = make_handler();
        let result = handler
            .call_tool("music_generate", json!({"prompt": "jazz"}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn praxis_evaluate_no_modules() {
        let handler = make_handler();
        let result = handler
            .call_tool("praxis_evaluate", json!({"action": "send_email"}))
            .await;
        assert!(!result.is_error);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["results"], json!([]));
        assert!(parsed["warning"].as_str().unwrap().contains("no praxis modules"));
    }

    #[tokio::test]
    async fn praxis_evaluate_with_safety_module() {
        use pares_agens_praxis::modules::safety::SafetyModule;
        let shell = Arc::new(ShellExecutor::new());
        let handler = RadixToolHandler::new(shell, PathBuf::from("/tmp"))
            .with_praxis_modules(vec![Box::new(SafetyModule::default())]);

        let result = handler
            .call_tool("praxis_evaluate", json!({"action": "send_email", "payload": {"recipients": 50}}))
            .await;
        assert!(!result.is_error);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["action"], "send_email");
        assert!(parsed["total_rules"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn praxis_list_no_modules() {
        let handler = make_handler();
        let result = handler
            .call_tool("praxis_list", json!({}))
            .await;
        assert!(!result.is_error);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["total_rules"], 0);
    }

    #[tokio::test]
    async fn praxis_list_with_modules() {
        use pares_agens_praxis::modules::safety::SafetyModule;
        let shell = Arc::new(ShellExecutor::new());
        let handler = RadixToolHandler::new(shell, PathBuf::from("/tmp"))
            .with_praxis_modules(vec![Box::new(SafetyModule::default())]);

        let result = handler
            .call_tool("praxis_list", json!({}))
            .await;
        assert!(!result.is_error);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert!(parsed["total_rules"].as_u64().unwrap() > 0);
        let modules = parsed["modules"].as_array().unwrap();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0]["name"], "safety");
    }

    #[tokio::test]
    async fn praxis_evaluate_missing_action() {
        let handler = make_handler();
        let result = handler
            .call_tool("praxis_evaluate", json!({}))
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("action"));
    }

    #[tokio::test]
    async fn praxis_list_tools_always_present() {
        let handler = make_handler();
        let tools = handler.list_tools().await;
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"praxis_evaluate"));
        assert!(names.contains(&"praxis_list"));
    }
}
