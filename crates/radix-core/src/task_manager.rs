//! Task manager — CRUD and lifecycle operations for [`Task`]s backed by PluresDB.
//!
//! The [`TaskManager`] stores tasks as JSON documents in a PluresDB
//! [`CrdtStore`], using the task ID as the node key. This gives us O(1)
//! lookups and CRDT merge semantics for distributed agents.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use pluresdb::CrdtStore;
use serde_json;
use tracing::{debug, info};
use uuid::Uuid;

use crate::task::{Assignment, CompletionCondition, Task, TaskStatus};

/// PluresDB actor ID used for task write operations.
const ACTOR: &str = "pares-radix-tasks";

/// Key prefix for task nodes in PluresDB.
const TASK_PREFIX: &str = "task:";

/// Manages task lifecycle backed by PluresDB.
pub struct TaskManager {
    store: Arc<CrdtStore>,
}

impl TaskManager {
    /// Create a new `TaskManager` backed by the given store.
    pub fn new(store: Arc<CrdtStore>) -> Self {
        Self { store }
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn task_key(id: &str) -> String {
        format!("{TASK_PREFIX}{id}")
    }

    fn store_task(&self, task: &Task) {
        let value = serde_json::to_value(task).expect("Task serializes to JSON");
        self.store.put(Self::task_key(&task.id), ACTOR, value);
    }

    fn load_task(&self, id: &str) -> Option<Task> {
        let record = self.store.get(Self::task_key(id))?;
        serde_json::from_value(record.data).ok()
    }

    /// Create a new task from a user request.
    pub fn create_task(
        &self,
        description: &str,
        chat_id: &str,
        conditions: Vec<CompletionCondition>,
    ) -> Task {
        let now = Self::now_ms();
        let task = Task {
            id: Uuid::new_v4().to_string(),
            description: description.to_string(),
            status: TaskStatus::Open,
            completion_conditions: conditions,
            subtasks: vec![],
            parent_task: None,
            assigned_to: Assignment::Unassigned,
            created_at: now,
            updated_at: now,
            created_by: format!("user:{chat_id}"),
            chat_id: Some(chat_id.to_string()),
            priority: 5,
            attempts: 0,
            last_evaluated_at: None,
            result: None,
            error: None,
        };
        info!(task_id = %task.id, "Created task: {}", description);
        self.store_task(&task);
        task
    }

    /// Create a subtask under a parent.
    pub fn create_subtask(
        &self,
        parent_id: &str,
        description: &str,
        conditions: Vec<CompletionCondition>,
    ) -> Option<Task> {
        let mut parent = self.load_task(parent_id)?;
        let now = Self::now_ms();
        let subtask = Task {
            id: Uuid::new_v4().to_string(),
            description: description.to_string(),
            status: TaskStatus::Open,
            completion_conditions: conditions,
            subtasks: vec![],
            parent_task: Some(parent_id.to_string()),
            assigned_to: Assignment::Unassigned,
            created_at: now,
            updated_at: now,
            created_by: format!("task:{parent_id}"),
            chat_id: parent.chat_id.clone(),
            priority: parent.priority,
            attempts: 0,
            last_evaluated_at: None,
            result: None,
            error: None,
        };
        parent.subtasks.push(subtask.id.clone());
        parent.updated_at = now;
        info!(
            task_id = %subtask.id,
            parent_id = %parent_id,
            "Created subtask: {}", description
        );
        self.store_task(&subtask);
        self.store_task(&parent);
        Some(subtask)
    }

    /// Get all open tasks (not completed, cancelled, or failed).
    pub fn open_tasks(&self) -> Vec<Task> {
        self.all_tasks()
            .into_iter()
            .filter(|t| !t.is_terminal())
            .collect()
    }

    /// Get tasks ready for evaluation: open/in-progress, assigned to self or unassigned,
    /// not fully delegated.
    pub fn evaluable_tasks(&self) -> Vec<Task> {
        self.all_tasks()
            .into_iter()
            .filter(|t| {
                matches!(t.status, TaskStatus::Open | TaskStatus::InProgress)
                    && matches!(t.assigned_to, Assignment::Self_ | Assignment::Unassigned)
            })
            .collect()
    }

    /// Update task status.
    pub fn update_status(&self, task_id: &str, status: TaskStatus) {
        if let Some(mut task) = self.load_task(task_id) {
            debug!(task_id, ?status, "Updating task status");
            task.status = status;
            task.updated_at = Self::now_ms();
            self.store_task(&task);
        }
    }

    /// Mark a completion condition as satisfied by index.
    pub fn satisfy_condition(&self, task_id: &str, condition_index: usize) {
        if let Some(mut task) = self.load_task(task_id) {
            if let Some(cond) = task.completion_conditions.get_mut(condition_index) {
                cond.satisfied = true;
                task.updated_at = Self::now_ms();
                debug!(task_id, condition_index, "Condition satisfied");
                self.store_task(&task);
            }
        }
    }

    /// Check if all conditions are satisfied; if so, auto-complete the task.
    /// Returns `true` if the task is now complete.
    pub fn check_completion(&self, task_id: &str) -> bool {
        if let Some(mut task) = self.load_task(task_id) {
            if task.all_conditions_satisfied() && !task.is_terminal() {
                task.status = TaskStatus::Completed;
                task.updated_at = Self::now_ms();
                info!(task_id, "Task auto-completed — all conditions satisfied");
                self.store_task(&task);
                return true;
            }
        }
        false
    }

    /// Record an evaluation attempt.
    pub fn record_evaluation(&self, task_id: &str, result: &str) {
        if let Some(mut task) = self.load_task(task_id) {
            task.attempts += 1;
            task.last_evaluated_at = Some(Self::now_ms());
            task.updated_at = Self::now_ms();
            debug!(
                task_id,
                attempts = task.attempts,
                "Evaluation recorded: {}",
                result
            );
            self.store_task(&task);
        }
    }

    /// Get task by ID.
    pub fn get_task(&self, task_id: &str) -> Option<Task> {
        self.load_task(task_id)
    }

    /// List tasks for a chat.
    pub fn tasks_for_chat(&self, chat_id: &str, include_completed: bool) -> Vec<Task> {
        self.all_tasks()
            .into_iter()
            .filter(|t| {
                t.chat_id.as_deref() == Some(chat_id) && (include_completed || !t.is_terminal())
            })
            .collect()
    }

    /// Cancel a task.
    pub fn cancel_task(&self, task_id: &str) {
        if let Some(mut task) = self.load_task(task_id) {
            task.status = TaskStatus::Cancelled;
            task.updated_at = Self::now_ms();
            info!(task_id, "Task cancelled");
            self.store_task(&task);
        }
    }

    /// Complete a task manually (requester ack).
    pub fn complete_task(&self, task_id: &str, result: Option<&str>) {
        if let Some(mut task) = self.load_task(task_id) {
            task.status = TaskStatus::Completed;
            task.result = result.map(|s| s.to_string());
            task.updated_at = Self::now_ms();
            // Also satisfy any RequesterAck conditions
            for cond in &mut task.completion_conditions {
                if matches!(
                    cond.condition_type,
                    crate::task::ConditionType::RequesterAck
                ) {
                    cond.satisfied = true;
                }
            }
            info!(task_id, "Task manually completed");
            self.store_task(&task);
        }
    }

    // -----------------------------------------------------------------------
    // Task Registry tool support
    // -----------------------------------------------------------------------

    /// Set task priority (1-10).
    pub fn set_priority(&self, task_id: &str, priority: u8) {
        if let Some(mut task) = self.load_task(task_id) {
            task.priority = priority.clamp(1, 10);
            task.updated_at = Self::now_ms();
            self.store_task(&task);
        }
    }

    /// Update task description.
    pub fn update_description(&self, task_id: &str, description: &str) {
        if let Some(mut task) = self.load_task(task_id) {
            task.description = description.to_string();
            task.updated_at = Self::now_ms();
            self.store_task(&task);
        }
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Load all tasks from the store by scanning the task prefix.
    fn all_tasks(&self) -> Vec<Task> {
        self.store
            .list()
            .into_iter()
            .filter(|record| record.id.starts_with(TASK_PREFIX))
            .filter_map(|record| serde_json::from_value::<Task>(record.data).ok())
            .collect()
    }
}

/// Render a compact grounding block of the agent's persisted open tasks for the
/// given chat. Returns `None` when there are no open tasks so callers never
/// inject an empty/noise block.
///
/// Combines chat-scoped open tasks with globally-open tasks (deduped by id) so
/// obligations survive conversation-history trimming and process restarts.
///
/// This is the SINGLE implementation shared by both model-invocation paths:
/// the Rust `ModelInvoker` SpineProcedure and the live reactive `.px` path
/// (via the `read_open_tasks_block` action). Keeping one function avoids the
/// two-file duplication ADR-0010 warns against.
pub fn render_open_tasks_block(manager: &TaskManager, chat_id: &str) -> Option<String> {
    // Chat-scoped open tasks first, then any other globally-open tasks.
    let mut tasks = manager.tasks_for_chat(chat_id, false);
    let mut seen: std::collections::HashSet<String> =
        tasks.iter().map(|t| t.id.clone()).collect();
    for t in manager.open_tasks() {
        if seen.insert(t.id.clone()) {
            tasks.push(t);
        }
    }

    if tasks.is_empty() {
        return None;
    }

    // Highest priority first (priority 1 = highest), then most recent.
    tasks.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then(b.created_at.cmp(&a.created_at))
    });

    let mut block = String::from(
        "## Your open tasks/commitments (durable, from the task store — treat as authoritative)\n",
    );
    for t in tasks.iter().take(25) {
        block.push_str(&format!(
            "- [{:?}] (p{}) {}\n",
            t.status, t.priority, t.description
        ));
    }
    block.push_str(
        "\nThese are your actual tracked obligations regardless of chat history length. \
        When asked what your tasks/commitments are, answer from this list.",
    );
    Some(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{CompletionCondition, ConditionType};
    use pluresdb::MemoryStorage;

    fn make_manager() -> TaskManager {
        let storage: Arc<dyn pluresdb::StorageEngine> = Arc::new(MemoryStorage::default());
        let store = CrdtStore::default().with_persistence(storage);
        TaskManager::new(Arc::new(store))
    }

    #[test]
    fn create_task_is_open() {
        let mgr = make_manager();
        let task = mgr.create_task(
            "Do something",
            "chat_123",
            vec![CompletionCondition {
                description: "done".into(),
                condition_type: ConditionType::RequesterAck,
                satisfied: false,
            }],
        );
        assert_eq!(task.status, TaskStatus::Open);
        assert_eq!(mgr.open_tasks().len(), 1);
    }

    #[test]
    fn create_subtask_links_parent() {
        let mgr = make_manager();
        let parent = mgr.create_task("Parent", "chat_1", vec![]);
        let sub = mgr.create_subtask(&parent.id, "Child", vec![]).unwrap();
        assert_eq!(sub.parent_task.as_deref(), Some(parent.id.as_str()));
        let reloaded = mgr.get_task(&parent.id).unwrap();
        assert!(reloaded.subtasks.contains(&sub.id));
    }

    #[test]
    fn satisfy_all_conditions_auto_completes() {
        let mgr = make_manager();
        let task = mgr.create_task(
            "Test",
            "chat_1",
            vec![
                CompletionCondition {
                    description: "a".into(),
                    condition_type: ConditionType::RequesterAck,
                    satisfied: false,
                },
                CompletionCondition {
                    description: "b".into(),
                    condition_type: ConditionType::RequesterAck,
                    satisfied: false,
                },
            ],
        );
        mgr.satisfy_condition(&task.id, 0);
        assert!(!mgr.check_completion(&task.id));
        mgr.satisfy_condition(&task.id, 1);
        assert!(mgr.check_completion(&task.id));
        let t = mgr.get_task(&task.id).unwrap();
        assert_eq!(t.status, TaskStatus::Completed);
    }

    #[test]
    fn delegated_tasks_skipped_in_evaluation() {
        let mgr = make_manager();
        let task = mgr.create_task("Delegated", "chat_1", vec![]);
        mgr.update_status(&task.id, TaskStatus::Delegated);
        assert!(mgr.evaluable_tasks().is_empty());
    }

    #[test]
    fn cancel_task() {
        let mgr = make_manager();
        let task = mgr.create_task("Cancel me", "chat_1", vec![]);
        mgr.cancel_task(&task.id);
        let t = mgr.get_task(&task.id).unwrap();
        assert_eq!(t.status, TaskStatus::Cancelled);
        assert!(mgr.open_tasks().is_empty());
    }

    #[test]
    fn tasks_for_chat_filters() {
        let mgr = make_manager();
        mgr.create_task("A", "chat_1", vec![]);
        mgr.create_task("B", "chat_2", vec![]);
        let tasks = mgr.tasks_for_chat("chat_1", false);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].description, "A");
    }
}
