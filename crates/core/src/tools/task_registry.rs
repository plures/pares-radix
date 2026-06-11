//! Task Registry Tool — built-in tool for conscious task management.
//!
//! The agent explicitly creates, completes, and queries tasks through this tool.
//! No heuristic scanning — the agent has full agency over its task lifecycle.
//!
//! Tool operations:
//!   - `task_create` — save a new task with description, priority, conditions
//!   - `task_complete` — mark a task done when constraints are met
//!   - `task_list` — return the current task queue (summary)
//!   - `task_get` — get full details for a specific task
//!   - `task_update` — update priority or description of an existing task
//!
//! The cerebellum auto-injects the task list into context so the agent
//! always sees its obligations without explicitly calling task_list.

use std::sync::Arc;

use serde_json::{json, Value};
use tracing::{info, warn};

use crate::model::ToolDefinition;
use crate::task::{CompletionCondition, ConditionType, TaskStatus};
use crate::task_manager::TaskManager;

/// Built-in tool names exposed to the model.
pub const TOOL_TASK_CREATE: &str = "task_create";
pub const TOOL_TASK_COMPLETE: &str = "task_complete";
pub const TOOL_TASK_LIST: &str = "task_list";
pub const TOOL_TASK_GET: &str = "task_get";
pub const TOOL_TASK_UPDATE: &str = "task_update";

/// All task registry tool names for matching.
pub const TASK_REGISTRY_TOOLS: &[&str] = &[
    TOOL_TASK_CREATE,
    TOOL_TASK_COMPLETE,
    TOOL_TASK_LIST,
    TOOL_TASK_GET,
    TOOL_TASK_UPDATE,
];

/// Task Registry — handles all task_* tool calls.
pub struct TaskRegistryTool {
    task_manager: Arc<TaskManager>,
}

impl TaskRegistryTool {
    pub fn new(task_manager: Arc<TaskManager>) -> Self {
        Self { task_manager }
    }

    /// Return all tool definitions for the task registry.
    pub fn tool_definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: TOOL_TASK_CREATE.into(),
                description: "Create a task to track work you've committed to. Use when you promise to do something, identify work that needs doing, or want to remember a follow-up.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "description": {
                            "type": "string",
                            "description": "What needs to be done. Be specific enough to act on later."
                        },
                        "priority": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 10,
                            "description": "Priority 1-10. 10=critical/immediate, 7-9=high/same session, 4-6=medium/soon, 1-3=low/when convenient. Default: 5."
                        },
                        "context": {
                            "type": "string",
                            "description": "Additional context about why this task exists or constraints on execution."
                        },
                        "completion_conditions": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of conditions that must be met for this task to be considered complete."
                        }
                    },
                    "required": ["description"]
                }),
            },
            ToolDefinition {
                name: TOOL_TASK_COMPLETE.into(),
                description: "Mark a task as complete. Use when you've fulfilled the commitment or the task is no longer relevant.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "The task ID to mark complete (first 8 chars from task list is sufficient)."
                        },
                        "reason": {
                            "type": "string",
                            "description": "Why this task is complete (what was done) or why it's no longer needed."
                        }
                    },
                    "required": ["task_id"]
                }),
            },
            ToolDefinition {
                name: TOOL_TASK_LIST.into(),
                description: "List all pending tasks. Usually not needed — the task list is auto-injected into your context. Use only when you need to verify task IDs or check full details.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "include_completed": {
                            "type": "boolean",
                            "description": "Include completed/cancelled tasks. Default: false (pending only)."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of tasks to return. Default: 20."
                        }
                    }
                }),
            },
            ToolDefinition {
                name: TOOL_TASK_GET.into(),
                description: "Get full details for a specific task including completion conditions and history.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "The task ID to retrieve."
                        }
                    },
                    "required": ["task_id"]
                }),
            },
            ToolDefinition {
                name: TOOL_TASK_UPDATE.into(),
                description: "Update a task's priority or description.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "The task ID to update."
                        },
                        "priority": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 10,
                            "description": "New priority level (1-10)."
                        },
                        "description": {
                            "type": "string",
                            "description": "Updated task description."
                        }
                    },
                    "required": ["task_id"]
                }),
            },
        ]
    }

    /// Handle a tool call by name, dispatching to the appropriate method.
    pub async fn call(&self, tool_name: &str, arguments: Value) -> String {
        match tool_name {
            TOOL_TASK_CREATE => self.handle_create(arguments).await,
            TOOL_TASK_COMPLETE => self.handle_complete(arguments).await,
            TOOL_TASK_LIST => self.handle_list(arguments).await,
            TOOL_TASK_GET => self.handle_get(arguments).await,
            TOOL_TASK_UPDATE => self.handle_update(arguments).await,
            _ => format!("Unknown task tool: {tool_name}"),
        }
    }

    /// Check if a tool name belongs to the task registry.
    pub fn handles_tool(name: &str) -> bool {
        TASK_REGISTRY_TOOLS.contains(&name)
    }

    /// Generate the context injection block for the agent's system prompt.
    ///
    /// Returns a formatted string showing pending tasks as a compact list.
    /// The cerebellum injects this automatically so the agent always sees its obligations.
    pub fn context_block(&self) -> String {
        let tasks = self.task_manager.evaluable_tasks();
        if tasks.is_empty() {
            return String::new();
        }

        let mut block = String::from("\n<pending_tasks>\n");
        for task in &tasks {
            let priority_icon = match task.priority {
                8..=10 => "🔴",
                6..=7 => "🟠",
                4..=5 => "🟡",
                _ => "🟢",
            };
            block.push_str(&format!(
                "  {} [{}] p{} — {}\n",
                priority_icon,
                &task.id[..task.id.len().min(8)],
                task.priority,
                task.description
            ));
        }
        block.push_str("</pending_tasks>\n");
        block.push_str("Use task_get(task_id) for details. Use task_complete(task_id, reason) when done.\n");
        block
    }

    // ─── Handlers ─────────────────────────────────────────────────

    async fn handle_create(&self, args: Value) -> String {
        let description = match args.get("description").and_then(|v| v.as_str()) {
            Some(d) => d.to_string(),
            None => return json!({"status": "error", "message": "'description' is required"}).to_string(),
        };

        let priority = args
            .get("priority")
            .and_then(|v| v.as_u64())
            .map(|p| p.clamp(1, 10) as u8)
            .unwrap_or(5);

        let conditions: Vec<CompletionCondition> = args
            .get("completion_conditions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|desc| CompletionCondition {
                        description: desc.to_string(),
                        condition_type: ConditionType::ModelEvaluation(desc.to_string()),
                        satisfied: false,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Create via TaskManager — uses "self" as chat_id for agent-created tasks
        let mut task = self.task_manager.create_task(&description, "self", conditions);

        // Update priority if non-default (create_task sets priority=5)
        if priority != 5 {
            self.task_manager.set_priority(&task.id, priority);
            task.priority = priority;
        }

        info!(
            task_id = %task.id,
            description = %description,
            priority = priority,
            "task_registry: created task"
        );

        json!({
            "status": "created",
            "task_id": task.id,
            "description": description,
            "priority": priority,
        })
        .to_string()
    }

    async fn handle_complete(&self, args: Value) -> String {
        let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return json!({"status": "error", "message": "'task_id' is required"}).to_string(),
        };

        let reason = args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Completed");

        // Resolve short IDs (first 8 chars)
        let resolved_id = self.resolve_task_id(task_id);
        let resolved_id = match resolved_id {
            Some(id) => id,
            None => {
                warn!(task_id = %task_id, "task_registry: task not found");
                return json!({
                    "status": "error",
                    "message": format!("Task '{}' not found", task_id),
                }).to_string();
            }
        };

        self.task_manager.complete_task(&resolved_id, Some(reason));

        info!(task_id = %resolved_id, reason = %reason, "task_registry: completed task");
        json!({
            "status": "completed",
            "task_id": resolved_id,
            "reason": reason,
        })
        .to_string()
    }

    async fn handle_list(&self, args: Value) -> String {
        let include_completed = args
            .get("include_completed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;

        let tasks = if include_completed {
            self.task_manager.open_tasks()
                .into_iter()
                .chain(self.task_manager.tasks_for_chat("self", true))
                .collect::<Vec<_>>()
        } else {
            self.task_manager.evaluable_tasks()
        };

        let tasks: Vec<_> = tasks.into_iter().take(limit).collect();
        let count = tasks.len();

        let task_summaries: Vec<Value> = tasks
            .iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "description": t.description,
                    "priority": t.priority,
                    "status": format!("{:?}", t.status),
                    "created_at": t.created_at,
                })
            })
            .collect();

        json!({
            "count": count,
            "tasks": task_summaries,
        })
        .to_string()
    }

    async fn handle_get(&self, args: Value) -> String {
        let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return json!({"status": "error", "message": "'task_id' is required"}).to_string(),
        };

        let resolved_id = self.resolve_task_id(task_id);
        let resolved_id = match resolved_id {
            Some(id) => id,
            None => {
                return json!({
                    "status": "error",
                    "message": format!("Task '{}' not found", task_id),
                }).to_string();
            }
        };

        match self.task_manager.get_task(&resolved_id) {
            Some(task) => {
                let conditions: Vec<Value> = task
                    .completion_conditions
                    .iter()
                    .map(|c| {
                        json!({
                            "description": c.description,
                            "satisfied": c.satisfied,
                        })
                    })
                    .collect();

                json!({
                    "id": task.id,
                    "description": task.description,
                    "priority": task.priority,
                    "status": format!("{:?}", task.status),
                    "chat_id": task.chat_id,
                    "created_at": task.created_at,
                    "updated_at": task.updated_at,
                    "attempts": task.attempts,
                    "completion_conditions": conditions,
                    "subtasks": task.subtasks,
                    "result": task.result,
                })
                .to_string()
            }
            None => {
                json!({
                    "status": "error",
                    "message": format!("Task '{}' not found", resolved_id),
                })
                .to_string()
            }
        }
    }

    async fn handle_update(&self, args: Value) -> String {
        let task_id = match args.get("task_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return json!({"status": "error", "message": "'task_id' is required"}).to_string(),
        };

        let resolved_id = match self.resolve_task_id(task_id) {
            Some(id) => id,
            None => {
                return json!({
                    "status": "error",
                    "message": format!("Task '{}' not found", task_id),
                }).to_string();
            }
        };

        let mut updated = false;

        if let Some(priority) = args.get("priority").and_then(|v| v.as_u64()) {
            let p = priority.clamp(1, 10) as u8;
            self.task_manager.set_priority(&resolved_id, p);
            updated = true;
        }

        if let Some(desc) = args.get("description").and_then(|v| v.as_str()) {
            self.task_manager.update_description(&resolved_id, desc);
            updated = true;
        }

        if updated {
            info!(task_id = %resolved_id, "task_registry: updated task");
            json!({
                "status": "updated",
                "task_id": resolved_id,
            })
            .to_string()
        } else {
            json!({
                "status": "no_change",
                "message": "No fields to update were provided",
            })
            .to_string()
        }
    }

    // ─── Helpers ──────────────────────────────────────────────────

    /// Resolve a potentially short task ID (first 8+ chars) to its full UUID.
    fn resolve_task_id(&self, short_id: &str) -> Option<String> {
        // Try exact match first
        if self.task_manager.get_task(short_id).is_some() {
            return Some(short_id.to_string());
        }

        // Try prefix match
        let all = self.task_manager.evaluable_tasks();
        let matches: Vec<_> = all.iter().filter(|t| t.id.starts_with(short_id)).collect();
        match matches.len() {
            1 => Some(matches[0].id.clone()),
            _ => None, // Ambiguous or not found
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pluresdb::{CrdtStore, MemoryStorage, StorageEngine};

    fn create_test_registry() -> TaskRegistryTool {
        let storage: Arc<dyn StorageEngine> = Arc::new(MemoryStorage::default());
        let store = Arc::new(CrdtStore::default().with_persistence(storage));
        let tm = Arc::new(TaskManager::new(store));
        TaskRegistryTool::new(tm)
    }

    #[tokio::test]
    async fn create_and_list_task() {
        let registry = create_test_registry();

        let result = registry
            .handle_create(json!({
                "description": "Fix the streaming bug",
                "priority": 8,
                "completion_conditions": ["Tokens stream in Telegram", "No errors in logs"]
            }))
            .await;

        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["status"], "created");
        assert_eq!(parsed["priority"], 8);
        let task_id = parsed["task_id"].as_str().unwrap().to_string();

        // List should find it
        let list_result = registry.handle_list(json!({})).await;
        let list: Value = serde_json::from_str(&list_result).unwrap();
        assert_eq!(list["count"], 1);
        assert_eq!(list["tasks"][0]["id"], task_id);
    }

    #[tokio::test]
    async fn complete_task_removes_from_pending() {
        let registry = create_test_registry();

        let result = registry
            .handle_create(json!({"description": "Deploy to prod"}))
            .await;
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let task_id = parsed["task_id"].as_str().unwrap();

        let complete_result = registry
            .handle_complete(json!({"task_id": task_id, "reason": "Deployed successfully"}))
            .await;
        let completed: Value = serde_json::from_str(&complete_result).unwrap();
        assert_eq!(completed["status"], "completed");

        // Should not appear in pending list
        let list_result = registry.handle_list(json!({})).await;
        let list: Value = serde_json::from_str(&list_result).unwrap();
        assert_eq!(list["count"], 0);
    }

    #[tokio::test]
    async fn short_id_resolution() {
        let registry = create_test_registry();

        let result = registry
            .handle_create(json!({"description": "Test short ID"}))
            .await;
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let task_id = parsed["task_id"].as_str().unwrap().to_string();
        let short_id = &task_id[..8];

        // Get by short ID
        let get_result = registry
            .handle_get(json!({"task_id": short_id}))
            .await;
        let got: Value = serde_json::from_str(&get_result).unwrap();
        assert_eq!(got["id"], task_id);
    }

    #[tokio::test]
    async fn context_block_shows_tasks() {
        let registry = create_test_registry();

        // Empty context when no tasks
        assert!(registry.context_block().is_empty());

        // Create a high-priority task
        registry
            .handle_create(json!({"description": "Fix critical bug", "priority": 9}))
            .await;

        let block = registry.context_block();
        assert!(block.contains("<pending_tasks>"));
        assert!(block.contains("🔴")); // priority 9 = red
        assert!(block.contains("Fix critical bug"));
        assert!(block.contains("task_complete"));
    }

    #[tokio::test]
    async fn update_priority() {
        let registry = create_test_registry();

        let result = registry
            .handle_create(json!({"description": "Low pri task", "priority": 2}))
            .await;
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let task_id = parsed["task_id"].as_str().unwrap();

        // Update to high priority
        registry
            .handle_update(json!({"task_id": task_id, "priority": 9}))
            .await;

        let get_result = registry.handle_get(json!({"task_id": task_id})).await;
        let got: Value = serde_json::from_str(&get_result).unwrap();
        assert_eq!(got["priority"], 9);
    }

    #[test]
    fn handles_tool_detection() {
        assert!(TaskRegistryTool::handles_tool("task_create"));
        assert!(TaskRegistryTool::handles_tool("task_complete"));
        assert!(TaskRegistryTool::handles_tool("task_list"));
        assert!(TaskRegistryTool::handles_tool("task_get"));
        assert!(TaskRegistryTool::handles_tool("task_update"));
        assert!(!TaskRegistryTool::handles_tool("unknown_tool"));
    }

    #[test]
    fn tool_definitions_valid() {
        let defs = TaskRegistryTool::tool_definitions();
        assert_eq!(defs.len(), 5);
        assert!(defs.iter().any(|d| d.name == "task_create"));
        assert!(defs.iter().any(|d| d.name == "task_complete"));
    }
}
