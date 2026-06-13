//! Dev-lifecycle action handlers for .px procedure execution.
//!
//! Provides the Rust implementations for actions called by `dev-lifecycle.px`:
//!
//! - `get_default_stages` — returns the default stage configuration
//! - `merge_stage_config` — merges task-specific overrides with defaults
//! - `find_next_stage` — finds the next pending stage after a given one
//! - `get_stage` — extracts a stage by name from a task record
//! - `update_stage_status` — updates a stage's status/output/attempts
//! - `format_stage_brief` — templates the subagent task brief string
//! - `collect_stage_outputs` — gathers outputs from all completed stages

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{debug, warn};

use crate::px_adapter::AsyncActionHandler;
use pares_radix_praxis::px::executor::ExecutionError;

/// Default stage definitions for pares-radix development lifecycle.
///
/// Each stage has:
/// - `name` — stage identifier
/// - `max_retries` — how many times to retry on failure
/// - `timeout_seconds` — max execution time per attempt
/// - `depends_on` — stages that must pass before this one starts
fn default_stages() -> Value {
    json!([
        {
            "name": "analyze",
            "max_retries": 1,
            "timeout_seconds": 300,
            "depends_on": [],
            "status": "pending",
            "attempts": 0,
            "output": null
        },
        {
            "name": "fix",
            "max_retries": 2,
            "timeout_seconds": 600,
            "depends_on": ["analyze"],
            "status": "pending",
            "attempts": 0,
            "output": null
        },
        {
            "name": "test",
            "max_retries": 2,
            "timeout_seconds": 600,
            "depends_on": ["fix"],
            "status": "pending",
            "attempts": 0,
            "output": null
        },
        {
            "name": "deploy",
            "max_retries": 1,
            "timeout_seconds": 300,
            "depends_on": ["test"],
            "status": "pending",
            "attempts": 0,
            "output": null
        },
        {
            "name": "verify",
            "max_retries": 2,
            "timeout_seconds": 300,
            "depends_on": ["deploy"],
            "status": "pending",
            "attempts": 0,
            "output": null
        }
    ])
}

/// Action handler for dev-lifecycle .px procedures.
///
/// Provides pure data-manipulation actions that the .px procedures call
/// to manage task state, stages, and brief generation.
pub struct DevLifecycleActionHandler;

impl DevLifecycleActionHandler {
    pub fn new() -> Self {
        Self
    }

    /// Returns the default stage configuration for a given repo.
    ///
    /// Params: `{repo: "plures/pares-radix"}`
    /// Returns: array of stage objects
    fn get_default_stages(&self, _params: &Value) -> Result<Value, ExecutionError> {
        // For now, all repos get the same default stages.
        // In the future, this could vary by repo.
        Ok(default_stages())
    }

    /// Merges task-specific stage overrides with defaults.
    ///
    /// Params: `{defaults: [...stages], overrides: {stage_name: {prompt: "...", ...}}}`
    /// Returns: merged array of stages with overrides applied
    fn merge_stage_config(&self, params: &Value) -> Result<Value, ExecutionError> {
        let defaults = params
            .get("defaults")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "merge_stage_config".into(),
                message: "missing or invalid 'defaults' array".into(),
            })?;

        let overrides = params.get("overrides").cloned().unwrap_or(Value::Null);

        let mut merged = Vec::new();
        for stage in defaults {
            let name = stage
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let mut merged_stage = stage.clone();

            // Apply overrides for this stage if they exist
            if let Some(stage_override) = overrides.get(name) {
                if let (Some(base), Some(over)) =
                    (merged_stage.as_object_mut(), stage_override.as_object())
                {
                    for (key, val) in over {
                        base.insert(key.clone(), val.clone());
                    }
                }
            }

            merged.push(merged_stage);
        }

        Ok(Value::Array(merged))
    }

    /// Finds the next pending stage after the given completed stage.
    ///
    /// Respects dependency ordering: only returns a stage whose dependencies
    /// are ALL in "passed" status.
    ///
    /// Params: `{task: {stages: [...]}, after: "stage_name"}`
    /// Returns: next stage name (string) or null if all done
    fn find_next_stage(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task = params.get("task").ok_or_else(|| ExecutionError::ActionFailed {
            action: "find_next_stage".into(),
            message: "missing 'task'".into(),
        })?;

        let stages = task
            .get("stages")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "find_next_stage".into(),
                message: "task has no 'stages' array".into(),
            })?;

        // Find the next stage that is "pending" and whose dependencies are all "passed"
        for stage in stages {
            let status = stage.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if status != "pending" {
                continue;
            }

            let depends_on = stage
                .get("depends_on")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            // Check all dependencies are passed
            let all_deps_passed = depends_on.iter().all(|dep| {
                let dep_name = dep.as_str().unwrap_or("");
                stages.iter().any(|s| {
                    s.get("name").and_then(|n| n.as_str()) == Some(dep_name)
                        && s.get("status").and_then(|st| st.as_str()) == Some("passed")
                })
            });

            if all_deps_passed {
                let name = stage.get("name").cloned().unwrap_or(Value::Null);
                debug!(next_stage = ?name, "find_next_stage: found ready stage");
                return Ok(name);
            }
        }

        // No more stages to run
        debug!("find_next_stage: all stages complete or blocked");
        Ok(Value::Null)
    }

    /// Extracts a stage by name from the task record.
    ///
    /// Params: `{task: {stages: [...]}, name: "stage_name"}`
    /// Returns: the stage object, or null if not found
    fn get_stage(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task = params.get("task").ok_or_else(|| ExecutionError::ActionFailed {
            action: "get_stage".into(),
            message: "missing 'task'".into(),
        })?;

        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "get_stage".into(),
                message: "missing 'name'".into(),
            })?;

        let stages = task
            .get("stages")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "get_stage".into(),
                message: "task has no 'stages' array".into(),
            })?;

        for stage in stages {
            if stage.get("name").and_then(|n| n.as_str()) == Some(name) {
                return Ok(stage.clone());
            }
        }

        debug!(stage_name = %name, "get_stage: not found");
        Ok(Value::Null)
    }

    /// Updates a stage's status, output, and attempts in the task record.
    ///
    /// Returns the full updated task record.
    ///
    /// Params: `{task: {...}, stage_name: "...", status: "...", output: "...", attempts: N}`
    /// Returns: updated task object
    fn update_stage_status(&self, params: &Value) -> Result<Value, ExecutionError> {
        let mut task = params
            .get("task")
            .cloned()
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "update_stage_status".into(),
                message: "missing 'task'".into(),
            })?;

        let stage_name = params
            .get("stage_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "update_stage_status".into(),
                message: "missing 'stage_name'".into(),
            })?;

        let new_status = params.get("status").cloned().unwrap_or(Value::Null);
        let output = params.get("output").cloned().unwrap_or(Value::Null);
        let attempts = params.get("attempts").cloned().unwrap_or(Value::Null);

        if let Some(stages) = task.get_mut("stages").and_then(|v| v.as_array_mut()) {
            for stage in stages.iter_mut() {
                if stage.get("name").and_then(|n| n.as_str()) == Some(stage_name) {
                    if let Some(obj) = stage.as_object_mut() {
                        if !new_status.is_null() {
                            obj.insert("status".to_string(), new_status.clone());
                        }
                        if !output.is_null() {
                            obj.insert("output".to_string(), output.clone());
                        }
                        if !attempts.is_null() {
                            obj.insert("attempts".to_string(), attempts.clone());
                        }
                    }
                    break;
                }
            }
        }

        // Update overall task status based on stage states
        if let Some(stages) = task.get("stages").and_then(|v| v.as_array()) {
            let all_passed = stages
                .iter()
                .all(|s| s.get("status").and_then(|v| v.as_str()) == Some("passed"));
            let any_failed = stages
                .iter()
                .any(|s| s.get("status").and_then(|v| v.as_str()) == Some("failed"));
            let any_blocked = stages
                .iter()
                .any(|s| s.get("status").and_then(|v| v.as_str()) == Some("blocked"));

            let task_status = if all_passed {
                "passed"
            } else if any_failed {
                "failed"
            } else if any_blocked {
                "blocked"
            } else {
                "running"
            };

            if let Some(obj) = task.as_object_mut() {
                obj.insert("status".to_string(), json!(task_status));
                if task_status == "failed" {
                    obj.insert("failed_stage".to_string(), json!(stage_name));
                }
            }
        }

        debug!(stage = %stage_name, "update_stage_status: updated");
        Ok(task)
    }

    /// Formats a subagent task brief from task/stage data.
    ///
    /// Params: `{task: {...}, stage: {...}, context: {...}, workdir: "..."}`
    ///   OR   `{task: {...}, summary: "..."}`
    /// Returns: formatted brief string
    fn format_stage_brief(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task = params.get("task").ok_or_else(|| ExecutionError::ActionFailed {
            action: "format_stage_brief".into(),
            message: "missing 'task'".into(),
        })?;

        // If this is a report_result call (has summary), format as notification
        if let Some(summary) = params.get("summary").and_then(|v| v.as_str()) {
            let task_id = task.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
            let description = task
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let status = task.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");

            let icon = match status {
                "passed" => "✅",
                "failed" => "❌",
                "blocked" => "🚫",
                _ => "📋",
            };

            let message = format!(
                "{icon} Task {task_id} ({status}): {description}\n\n{summary}"
            );
            return Ok(json!(message));
        }

        // Stage brief formatting
        let stage = params.get("stage").unwrap_or(&Value::Null);
        let context = params.get("context").unwrap_or(&Value::Null);
        let workdir = params
            .get("workdir")
            .and_then(|v| v.as_str())
            .or_else(|| task.get("workdir").and_then(|v| v.as_str()))
            .unwrap_or(".");

        let task_id = task.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
        let description = task
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let repo = task
            .get("repo")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let stage_name = stage
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let stage_prompt = stage.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
        let timeout = stage
            .get("timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(600);

        let mut brief = format!(
            "## Dev Lifecycle: {stage_name} stage\n\
             **Task ID:** {task_id}\n\
             **Task:** {description}\n\
             **Repo:** {repo}\n\
             **Working dir:** {workdir}\n\
             **Timeout:** {timeout}s\n"
        );

        if !stage_prompt.is_empty() {
            brief.push_str(&format!("\n### Instructions\n{stage_prompt}\n"));
        }

        // Append context from prior stages
        if let Some(ctx_obj) = context.as_object() {
            if !ctx_obj.is_empty() {
                brief.push_str("\n### Context from prior stages\n");
                for (stage_name, output) in ctx_obj {
                    let output_str = match output {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    brief.push_str(&format!("**{stage_name}:** {output_str}\n"));
                }
            }
        }

        Ok(json!(brief))
    }

    /// Collects outputs from all completed stages into a summary.
    ///
    /// Params: `{task: {stages: [...]}}`
    /// Returns: summary string or object with stage_name → output mappings
    fn collect_stage_outputs(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task = params.get("task").ok_or_else(|| ExecutionError::ActionFailed {
            action: "collect_stage_outputs".into(),
            message: "missing 'task'".into(),
        })?;

        let stages = task
            .get("stages")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "collect_stage_outputs".into(),
                message: "task has no 'stages' array".into(),
            })?;

        let mut summary_parts = Vec::new();
        for stage in stages {
            let name = stage.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let status = stage.get("status").and_then(|v| v.as_str()).unwrap_or("?");
            let output = stage
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or("(no output)");
            let attempts = stage.get("attempts").and_then(|v| v.as_u64()).unwrap_or(0);

            let icon = match status {
                "passed" => "✅",
                "failed" => "❌",
                "blocked" => "🚫",
                "running" => "🔄",
                "pending" => "⏳",
                "skipped" => "⏭️",
                _ => "❓",
            };

            summary_parts.push(format!(
                "{icon} **{name}** ({status}, {attempts} attempts): {output}"
            ));
        }

        Ok(json!(summary_parts.join("\n")))
    }
}

/// Actions handled by the dev-lifecycle handler.
const DEV_LIFECYCLE_ACTIONS: &[&str] = &[
    "get_default_stages",
    "merge_stage_config",
    "find_next_stage",
    "get_stage",
    "update_stage_status",
    "format_stage_brief",
    "collect_stage_outputs",
];

/// Check if an action name is handled by the dev-lifecycle handler.
pub fn is_dev_lifecycle_action(action: &str) -> bool {
    DEV_LIFECYCLE_ACTIONS.contains(&action)
}

#[async_trait]
impl AsyncActionHandler for DevLifecycleActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "get_default_stages" => self.get_default_stages(params),
            "merge_stage_config" => self.merge_stage_config(params),
            "find_next_stage" => self.find_next_stage(params),
            "get_stage" => self.get_stage(params),
            "update_stage_status" => self.update_stage_status(params),
            "format_stage_brief" => self.format_stage_brief(params),
            "collect_stage_outputs" => self.collect_stage_outputs(params),
            _ => {
                warn!(action = %name, "dev_lifecycle_actions: unknown action");
                Err(ExecutionError::ActionFailed {
                    action: name.to_string(),
                    message: "not a dev-lifecycle action".into(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handler() -> DevLifecycleActionHandler {
        DevLifecycleActionHandler::new()
    }

    #[tokio::test]
    async fn get_default_stages_returns_five() {
        let h = handler();
        let result = h.call("get_default_stages", &json!({"repo": "plures/pares-radix"})).await.unwrap();
        let stages = result.as_array().unwrap();
        assert_eq!(stages.len(), 5);
        assert_eq!(stages[0]["name"], "analyze");
        assert_eq!(stages[4]["name"], "verify");
    }

    #[tokio::test]
    async fn merge_stage_config_applies_overrides() {
        let h = handler();
        let defaults = default_stages();
        let params = json!({
            "defaults": defaults,
            "overrides": {
                "fix": {"prompt": "Fix the bug", "timeout_seconds": 900},
                "test": {"prompt": "Run cargo test"}
            }
        });
        let result = h.call("merge_stage_config", &params).await.unwrap();
        let stages = result.as_array().unwrap();
        assert_eq!(stages[1]["prompt"], "Fix the bug");
        assert_eq!(stages[1]["timeout_seconds"], 900);
        assert_eq!(stages[2]["prompt"], "Run cargo test");
        // analyze should NOT have a prompt
        assert!(stages[0].get("prompt").is_none());
    }

    #[tokio::test]
    async fn find_next_stage_respects_dependencies() {
        let h = handler();
        let task = json!({
            "stages": [
                {"name": "analyze", "status": "passed", "depends_on": []},
                {"name": "fix", "status": "passed", "depends_on": ["analyze"]},
                {"name": "test", "status": "pending", "depends_on": ["fix"]},
                {"name": "deploy", "status": "pending", "depends_on": ["test"]},
                {"name": "verify", "status": "pending", "depends_on": ["deploy"]}
            ]
        });
        let result = h.call("find_next_stage", &json!({"task": task, "after": "fix"})).await.unwrap();
        assert_eq!(result, "test");
    }

    #[tokio::test]
    async fn find_next_stage_returns_null_when_all_done() {
        let h = handler();
        let task = json!({
            "stages": [
                {"name": "analyze", "status": "passed", "depends_on": []},
                {"name": "fix", "status": "passed", "depends_on": ["analyze"]},
                {"name": "test", "status": "passed", "depends_on": ["fix"]},
                {"name": "deploy", "status": "passed", "depends_on": ["test"]},
                {"name": "verify", "status": "passed", "depends_on": ["deploy"]}
            ]
        });
        let result = h.call("find_next_stage", &json!({"task": task, "after": "verify"})).await.unwrap();
        assert_eq!(result, Value::Null);
    }

    #[tokio::test]
    async fn find_next_stage_blocks_on_unmet_deps() {
        let h = handler();
        let task = json!({
            "stages": [
                {"name": "analyze", "status": "passed", "depends_on": []},
                {"name": "fix", "status": "failed", "depends_on": ["analyze"]},
                {"name": "test", "status": "pending", "depends_on": ["fix"]},
                {"name": "deploy", "status": "pending", "depends_on": ["test"]},
                {"name": "verify", "status": "pending", "depends_on": ["deploy"]}
            ]
        });
        // test depends on fix which failed, so nothing is ready
        let result = h.call("find_next_stage", &json!({"task": task, "after": "fix"})).await.unwrap();
        assert_eq!(result, Value::Null);
    }

    #[tokio::test]
    async fn get_stage_finds_by_name() {
        let h = handler();
        let task = json!({
            "stages": [
                {"name": "analyze", "status": "passed"},
                {"name": "fix", "status": "running"}
            ]
        });
        let result = h.call("get_stage", &json!({"task": task, "name": "fix"})).await.unwrap();
        assert_eq!(result["status"], "running");
    }

    #[tokio::test]
    async fn get_stage_returns_null_for_missing() {
        let h = handler();
        let task = json!({"stages": [{"name": "analyze", "status": "passed"}]});
        let result = h.call("get_stage", &json!({"task": task, "name": "nonexistent"})).await.unwrap();
        assert_eq!(result, Value::Null);
    }

    #[tokio::test]
    async fn update_stage_status_updates_correctly() {
        let h = handler();
        let task = json!({
            "stages": [
                {"name": "analyze", "status": "running", "attempts": 1, "output": null, "depends_on": []},
                {"name": "fix", "status": "pending", "attempts": 0, "output": null, "depends_on": ["analyze"]}
            ],
            "status": "running"
        });
        let params = json!({
            "task": task,
            "stage_name": "analyze",
            "status": "passed",
            "output": "Analysis complete",
            "attempts": 1
        });
        let result = h.call("update_stage_status", &params).await.unwrap();
        let stages = result["stages"].as_array().unwrap();
        assert_eq!(stages[0]["status"], "passed");
        assert_eq!(stages[0]["output"], "Analysis complete");
        // Task status should be "running" since fix is still pending
        assert_eq!(result["status"], "running");
    }

    #[tokio::test]
    async fn collect_stage_outputs_formats_summary() {
        let h = handler();
        let task = json!({
            "stages": [
                {"name": "analyze", "status": "passed", "attempts": 1, "output": "Found 3 issues"},
                {"name": "fix", "status": "passed", "attempts": 2, "output": "Fixed all issues"},
                {"name": "test", "status": "pending", "attempts": 0, "output": null}
            ]
        });
        let result = h.call("collect_stage_outputs", &json!({"task": task})).await.unwrap();
        let summary = result.as_str().unwrap();
        assert!(summary.contains("analyze"));
        assert!(summary.contains("Found 3 issues"));
        assert!(summary.contains("fix"));
        assert!(summary.contains("Fixed all issues"));
    }

    #[tokio::test]
    async fn format_stage_brief_produces_markdown() {
        let h = handler();
        let params = json!({
            "task": {
                "id": "TASK-001",
                "description": "Fix the parser",
                "repo": "plures/pares-radix",
                "workdir": "/projects/pares-radix"
            },
            "stage": {
                "name": "fix",
                "prompt": "Fix the parser bug",
                "timeout_seconds": 600
            },
            "context": {
                "analyze": "Found 3 issues in parser.rs"
            },
            "workdir": "/projects/pares-radix"
        });
        let result = h.call("format_stage_brief", &params).await.unwrap();
        let brief = result.as_str().unwrap();
        assert!(brief.contains("TASK-001"));
        assert!(brief.contains("fix stage"));
        assert!(brief.contains("Fix the parser bug"));
        assert!(brief.contains("Found 3 issues"));
    }
}
