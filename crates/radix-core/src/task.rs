//! Task model — Praxis-driven task tracking with completion conditions.
//!
//! Every user request creates a [`Task`] in PluresDB. Tasks have measurable
//! [`CompletionCondition`]s. Task evaluation is driven by `.px` procedures
//! triggered via SpineEvent::Timer.

use serde::{Deserialize, Serialize};

/// A tracked unit of work originating from a user request or system event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier (UUID v4).
    pub id: String,
    /// Human-readable description of what needs to be done.
    pub description: String,
    /// Current lifecycle status.
    pub status: TaskStatus,
    /// Measurable conditions that must all be satisfied for completion.
    pub completion_conditions: Vec<CompletionCondition>,
    /// IDs of child tasks.
    pub subtasks: Vec<String>,
    /// ID of parent task (if this is a subtask).
    pub parent_task: Option<String>,
    /// Who is responsible for this task.
    pub assigned_to: Assignment,
    /// Unix timestamp (ms) when the task was created.
    pub created_at: u64,
    /// Unix timestamp (ms) of last update.
    pub updated_at: u64,
    /// Origin: `"user:{chat_id}"`, `"system"`, or `"task:{parent_id}"`.
    pub created_by: String,
    /// Which chat this task originated from.
    pub chat_id: Option<String>,
    /// Priority 1 (low) to 10 (high).
    pub priority: u8,
    /// How many times the cerebellum has evaluated this task.
    pub attempts: u32,
    /// Unix timestamp (ms) of last evaluation.
    pub last_evaluated_at: Option<u64>,
    /// Outcome when completed.
    pub result: Option<String>,
    /// Error description if failed.
    pub error: Option<String>,
}

/// Lifecycle status of a task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    /// Not yet started.
    Open,
    /// Being worked on.
    InProgress,
    /// Waiting for user input or external dependency.
    Blocked,
    /// Assigned to subagent(s).
    Delegated,
    /// All completion conditions satisfied.
    Completed,
    /// Could not complete.
    Failed,
    /// User cancelled.
    Cancelled,
}

/// A single measurable condition for task completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionCondition {
    /// Human-readable description.
    pub description: String,
    /// How this condition is evaluated.
    pub condition_type: ConditionType,
    /// Whether this condition has been satisfied.
    pub satisfied: bool,
}

/// How a completion condition is evaluated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionType {
    /// A programmatic check, e.g. `"file_exists:/path"` or `"tests_pass:/project"`.
    Check(String),
    /// Requires the requester to acknowledge completion.
    RequesterAck,
    /// A subtask must be complete. Contains the subtask ID.
    SubtaskComplete(String),
    /// A custom condition evaluated by the model (natural language).
    ModelEvaluation(String),
}

/// Who is responsible for executing the task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Assignment {
    /// This agent handles it.
    #[serde(rename = "self")]
    Self_,
    /// Delegated to a named subagent.
    Subagent(String),
    /// Waiting for user action.
    User,
    /// Not yet assigned.
    Unassigned,
}

impl Task {
    /// Returns `true` if all completion conditions are satisfied.
    pub fn all_conditions_satisfied(&self) -> bool {
        !self.completion_conditions.is_empty()
            && self.completion_conditions.iter().all(|c| c.satisfied)
    }

    /// Returns `true` if the task is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(conditions: Vec<CompletionCondition>) -> Task {
        Task {
            id: "test-1".into(),
            description: "Test task".into(),
            status: TaskStatus::Open,
            completion_conditions: conditions,
            subtasks: vec![],
            parent_task: None,
            assigned_to: Assignment::Unassigned,
            created_at: 0,
            updated_at: 0,
            created_by: "system".into(),
            chat_id: None,
            priority: 5,
            attempts: 0,
            last_evaluated_at: None,
            result: None,
            error: None,
        }
    }

    #[test]
    fn empty_conditions_not_satisfied() {
        let t = make_task(vec![]);
        assert!(!t.all_conditions_satisfied());
    }

    #[test]
    fn all_satisfied() {
        let t = make_task(vec![CompletionCondition {
            description: "done".into(),
            condition_type: ConditionType::RequesterAck,
            satisfied: true,
        }]);
        assert!(t.all_conditions_satisfied());
    }

    #[test]
    fn partial_not_satisfied() {
        let t = make_task(vec![
            CompletionCondition {
                description: "a".into(),
                condition_type: ConditionType::RequesterAck,
                satisfied: true,
            },
            CompletionCondition {
                description: "b".into(),
                condition_type: ConditionType::RequesterAck,
                satisfied: false,
            },
        ]);
        assert!(!t.all_conditions_satisfied());
    }

    #[test]
    fn terminal_states() {
        let mut t = make_task(vec![]);
        assert!(!t.is_terminal());
        t.status = TaskStatus::Completed;
        assert!(t.is_terminal());
        t.status = TaskStatus::Failed;
        assert!(t.is_terminal());
        t.status = TaskStatus::Cancelled;
        assert!(t.is_terminal());
        t.status = TaskStatus::InProgress;
        assert!(!t.is_terminal());
    }

    #[test]
    fn serde_roundtrip() {
        let t = make_task(vec![CompletionCondition {
            description: "check".into(),
            condition_type: ConditionType::Check("file_exists:/tmp/x".into()),
            satisfied: false,
        }]);
        let json = serde_json::to_string(&t).unwrap();
        let t2: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(t2.id, "test-1");
        assert!(!t2.completion_conditions[0].satisfied);
    }
}
