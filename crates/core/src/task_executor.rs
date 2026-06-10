//! Autonomous task executor — bridges TaskManager with the agent loop.
//!
//! When the heartbeat detects evaluable tasks, it invokes the [`TaskExecutor`]
//! which feeds the task back into the agent as a synthetic event. This enables
//! the agent to work on tasks between user messages.
//!
//! # Architecture
//!
//! ```text
//! Heartbeat tick
//!    ↓
//! TaskManager.evaluable_tasks()
//!    ↓ (tasks found)
//! TaskExecutor.execute_next()
//!    ↓
//! EventSpine.emit_inbound_message(task context)
//!    ↓
//! Agent.handle_event() — model works on the task
//!    ↓
//! TaskManager.record_evaluation() / satisfy_condition()
//! ```

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{debug, info, warn};

use crate::event_spine::EventSpineHandle;
use crate::state::StateStore;
use crate::task::{Task, TaskStatus};
use crate::task_manager::TaskManager;

/// Minimum time between evaluations of the same task (prevents rapid-fire loops).
const MIN_EVALUATION_INTERVAL_MS: u64 = 60_000; // 1 minute

/// Maximum concurrent task evaluations per heartbeat tick.
const MAX_EVALUATIONS_PER_TICK: usize = 1;

/// State key for tracking autonomous execution metadata.
const STATE_KEY_LAST_EXEC: &str = "task_executor/last_execution";

/// Drives autonomous task execution through the event spine.
pub struct TaskExecutor {
    task_manager: Arc<TaskManager>,
    state: Arc<dyn StateStore>,
    event_spine: Option<EventSpineHandle>,
}

impl TaskExecutor {
    /// Create a new task executor.
    pub fn new(task_manager: Arc<TaskManager>, state: Arc<dyn StateStore>) -> Self {
        Self {
            task_manager,
            state,
            event_spine: None,
        }
    }

    /// Attach an event spine handle for emitting task events.
    #[must_use]
    pub fn with_event_spine(mut self, spine: EventSpineHandle) -> Self {
        self.event_spine = Some(spine);
        self
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Check for evaluable tasks and execute the highest-priority one.
    ///
    /// Returns `true` if a task was dispatched for execution.
    pub async fn try_execute_next(&self) -> bool {
        let tasks = self.task_manager.evaluable_tasks();
        if tasks.is_empty() {
            debug!("task_executor: no evaluable tasks");
            return false;
        }

        let now = Self::now_ms();

        // Filter to tasks that haven't been evaluated too recently
        let ready_tasks: Vec<&Task> = tasks
            .iter()
            .filter(|t| {
                t.last_evaluated_at
                    .map(|last| now - last > MIN_EVALUATION_INTERVAL_MS)
                    .unwrap_or(true) // never evaluated = ready
            })
            .collect();

        if ready_tasks.is_empty() {
            debug!("task_executor: all tasks evaluated recently, cooling down");
            return false;
        }

        // Pick highest priority (lowest number = highest priority), then oldest
        let next = ready_tasks
            .iter()
            .min_by_key(|t| (t.priority, t.created_at))
            .unwrap();

        self.dispatch_task(next).await
    }

    /// Dispatch a task to the agent via the event spine.
    async fn dispatch_task(&self, task: &Task) -> bool {
        let Some(spine) = &self.event_spine else {
            warn!("task_executor: no event spine — cannot dispatch task");
            return false;
        };

        // Mark task as in-progress
        self.task_manager
            .update_status(&task.id, TaskStatus::InProgress);
        self.task_manager.record_evaluation(&task.id, "dispatched");

        // Build the task prompt for the model
        let conditions_text = if task.completion_conditions.is_empty() {
            String::from("No specific conditions — use your judgment.")
        } else {
            task.completion_conditions
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let status = if c.satisfied { "✅" } else { "⏳" };
                    format!("  {}. {} {} ({:?})", i + 1, status, c.description, c.condition_type)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let prompt = format!(
            "[autonomous-task] Execute this task:\n\
             Task: {}\n\
             ID: {}\n\
             Priority: {}\n\
             Attempts: {}\n\
             Conditions:\n{}\n\n\
             Work on this task using available tools. When complete, indicate which \
             conditions are satisfied. If blocked, explain why.",
            task.description, task.id, task.priority, task.attempts, conditions_text
        );

        info!(
            task_id = %task.id,
            description = %task.description,
            "task_executor: dispatching task to agent"
        );

        spine.emit_inbound_message(0, "task_executor", &prompt);

        // Record execution timestamp
        self.state
            .set(
                STATE_KEY_LAST_EXEC,
                serde_json::json!({
                    "task_id": task.id,
                    "timestamp": Self::now_ms(),
                }),
            )
            .await;

        true
    }

    /// Get the number of evaluable tasks.
    pub fn pending_count(&self) -> usize {
        self.task_manager.evaluable_tasks().len()
    }

    /// Get a summary of pending work for status display.
    pub fn status_summary(&self) -> String {
        let evaluable = self.task_manager.evaluable_tasks();
        if evaluable.is_empty() {
            return String::from("No pending tasks.");
        }

        let mut summary = format!("{} task(s) pending:\n", evaluable.len());
        for task in evaluable.iter().take(5) {
            let short_id = &task.id[..8.min(task.id.len())];
            summary.push_str(&format!(
                "  • [{}] {} (priority: {})\n",
                short_id, task.description, task.priority
            ));
        }
        if evaluable.len() > 5 {
            summary.push_str(&format!("  ... and {} more\n", evaluable.len() - 5));
        }
        summary
    }
}

/// Extract task commitments from a model response.
///
/// Looks for patterns like "I will...", "I'll...", "Let me...", "I'm going to..."
/// that indicate the agent made a promise to do something.
///
/// Returns a list of extracted commitment descriptions.
pub fn extract_commitments(response: &str) -> Vec<String> {
    let mut commitments = Vec::new();

    // Pattern: numbered lists starting with action verbs
    // "1. Diagnose why..."
    // "2. Fix the..."
    let numbered_re = regex_lite::Regex::new(
        r"(?m)^\s*\d+\.\s*((?:Diagnose|Fix|Implement|Write|Create|Update|Check|Verify|Build|Deploy|Configure|Set up|Refactor|Optimize|Debug|Test|Add|Remove|Migrate|Install|Resolve)\b[^\n]{10,})"
    ).unwrap();

    for cap in numbered_re.captures_iter(response) {
        if let Some(m) = cap.get(1) {
            commitments.push(m.as_str().trim().to_string());
        }
    }

    // Pattern: "I will [verb]..." / "I'll [verb]..."
    let will_re = regex_lite::Regex::new(
        r"(?i)I(?:'ll| will)\s+((?:diagnose|fix|implement|write|create|update|check|verify|build|deploy|configure|set up|refactor|optimize|debug|test|add|remove|migrate|install|resolve)\b[^.!?\n]{10,})"
    ).unwrap();

    for cap in will_re.captures_iter(response) {
        if let Some(m) = cap.get(1) {
            let commitment = m.as_str().trim().to_string();
            // Deduplicate against numbered items
            if !commitments.iter().any(|c| c.contains(&commitment[..commitment.len().min(30)])) {
                commitments.push(commitment);
            }
        }
    }

    commitments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_numbered_commitments() {
        let response = "Acknowledged. I will:\n\
            1. Diagnose why Telegram streaming is not working\n\
            2. Fix the streaming implementation\n\
            3. Verify streaming is restored";
        let commitments = extract_commitments(response);
        assert_eq!(commitments.len(), 3);
        assert!(commitments[0].starts_with("Diagnose"));
        assert!(commitments[1].starts_with("Fix"));
        assert!(commitments[2].starts_with("Verify"));
    }

    #[test]
    fn extract_will_commitments() {
        let response = "I'll fix the streaming bug and then I will verify it works end-to-end.";
        let commitments = extract_commitments(response);
        assert!(commitments.len() >= 1);
        assert!(commitments.iter().any(|c| c.contains("fix")));
    }

    #[test]
    fn no_false_positives() {
        let response = "The weather is nice today. I think we should go outside.";
        let commitments = extract_commitments(response);
        assert!(commitments.is_empty());
    }

    #[test]
    fn short_responses_ignored() {
        let response = "I will fix it."; // too short (< 10 chars after verb)
        let commitments = extract_commitments(response);
        assert!(commitments.is_empty());
    }
}
