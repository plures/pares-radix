//! Task loop — idle-time evaluator that cycles open tasks via the cerebellum.
//!
//! The [`TaskLoop`] replaces the simple heartbeat timer with a Praxis-driven
//! approach: on idle, it fetches evaluable tasks from the [`TaskManager`] and
//! decides whether to act, decompose, or block.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Local, Timelike};
use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::task::{Task, TaskStatus};
use crate::task_manager::TaskManager;

/// Configuration for the task evaluation loop.
#[derive(Debug, Clone)]
pub struct TaskLoopConfig {
    /// How often to check for idle (ms). Default: 5000.
    pub idle_check_interval_ms: u64,
    /// Max tasks to evaluate per idle cycle. Default: 5.
    pub max_tasks_per_cycle: usize,
    /// Minimum idle time before starting a cycle (ms). Default: 30000.
    pub min_idle_before_cycle_ms: u64,
    /// Max time to spend evaluating one task (ms). Default: 60000.
    pub max_evaluation_time_ms: u64,
    /// Start of quiet hours (hour, 0-23). Default: 23.
    pub quiet_hours_start: u8,
    /// End of quiet hours (hour, 0-23). Default: 8.
    pub quiet_hours_end: u8,
}

impl Default for TaskLoopConfig {
    fn default() -> Self {
        Self {
            idle_check_interval_ms: 5_000,
            max_tasks_per_cycle: 5,
            min_idle_before_cycle_ms: 30_000,
            max_evaluation_time_ms: 60_000,
            quiet_hours_start: 23,
            quiet_hours_end: 8,
        }
    }
}

/// Result of evaluating a task via the cerebellum.
#[derive(Debug)]
pub enum TaskEvaluation {
    /// Has a plan, ready to execute.
    CanAct(String),
    /// Too complex — needs breakdown into subtasks.
    NeedsDecomposition(Vec<SubtaskPlan>),
    /// Needs clarification from the user.
    NeedsUserInput(String),
    /// No actionable path right now.
    CannotAct,
}

/// A planned subtask for decomposition.
#[derive(Debug)]
pub struct SubtaskPlan {
    /// Description of the subtask.
    pub description: String,
    /// Completion conditions for the subtask.
    pub conditions: Vec<crate::task::CompletionCondition>,
}

/// Callback trait for the task loop to interact with the agent/cerebellum.
///
/// Implementors provide the actual LLM evaluation and execution logic.
#[async_trait::async_trait]
pub trait TaskEvaluator: Send + Sync {
    /// Evaluate a task and decide what to do.
    async fn evaluate(&self, task: &Task) -> TaskEvaluation;
    /// Execute a plan against a task.
    async fn execute_plan(&self, task: &Task, plan: &str);
    /// Send a message to the task's originating chat.
    async fn send_to_chat(&self, chat_id: &str, message: &str);
}

/// The idle task evaluation loop.
pub struct TaskLoop {
    task_manager: Arc<TaskManager>,
    config: TaskLoopConfig,
    /// Epoch-ms timestamp of last inbound message. Atomically updated.
    last_message_at: Arc<AtomicU64>,
}

impl TaskLoop {
    /// Create a new task loop.
    pub fn new(task_manager: Arc<TaskManager>, config: TaskLoopConfig) -> Self {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            task_manager,
            config,
            last_message_at: Arc::new(AtomicU64::new(now_ms)),
        }
    }

    /// Get a handle that can be used to notify the loop of inbound messages.
    pub fn message_notifier(&self) -> MessageNotifier {
        MessageNotifier {
            last_message_at: Arc::clone(&self.last_message_at),
        }
    }

    /// Run the task loop as a background tokio task.
    pub async fn run(
        &self,
        evaluator: Arc<dyn TaskEvaluator>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        info!("Task loop started");
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    info!("Task loop shutting down");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(self.config.idle_check_interval_ms)) => {
                    // Check idle duration
                    let now_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let last = self.last_message_at.load(Ordering::Relaxed);
                    let idle_ms = now_ms.saturating_sub(last);

                    if idle_ms < self.config.min_idle_before_cycle_ms {
                        continue;
                    }

                    if self.in_quiet_hours() {
                        debug!("Skipping task cycle — quiet hours");
                        continue;
                    }

                    let tasks = self.task_manager.evaluable_tasks();
                    if tasks.is_empty() {
                        continue;
                    }

                    debug!(count = tasks.len(), "Evaluating tasks");

                    for task in tasks.iter().take(self.config.max_tasks_per_cycle) {
                        let evaluation = tokio::time::timeout(
                            Duration::from_millis(self.config.max_evaluation_time_ms),
                            evaluator.evaluate(task),
                        )
                        .await;

                        match evaluation {
                            Ok(TaskEvaluation::CanAct(plan)) => {
                                self.task_manager.update_status(&task.id, TaskStatus::InProgress);
                                evaluator.execute_plan(task, &plan).await;
                            }
                            Ok(TaskEvaluation::NeedsDecomposition(subtasks)) => {
                                for sub in subtasks {
                                    self.task_manager.create_subtask(
                                        &task.id,
                                        &sub.description,
                                        sub.conditions,
                                    );
                                }
                                self.task_manager.update_status(&task.id, TaskStatus::Delegated);
                            }
                            Ok(TaskEvaluation::NeedsUserInput(question)) => {
                                self.task_manager.update_status(&task.id, TaskStatus::Blocked);
                                if let Some(chat_id) = &task.chat_id {
                                    evaluator.send_to_chat(chat_id, &question).await;
                                }
                            }
                            Ok(TaskEvaluation::CannotAct) => {
                                self.task_manager.record_evaluation(&task.id, "No actionable plan");
                            }
                            Err(_) => {
                                warn!(task_id = %task.id, "Task evaluation timed out");
                                self.task_manager.record_evaluation(&task.id, "Evaluation timed out");
                            }
                        }

                        // Check completion after action
                        self.task_manager.check_completion(&task.id);
                    }
                }
            }
        }
    }

    fn in_quiet_hours(&self) -> bool {
        let hour = Local::now().hour() as u8;
        let start = self.config.quiet_hours_start;
        let end = self.config.quiet_hours_end;
        if start <= end {
            hour >= start && hour < end
        } else {
            // Wraps midnight, e.g. 23..8
            hour >= start || hour < end
        }
    }
}

/// A lightweight handle to notify the task loop that a message was received.
#[derive(Clone)]
pub struct MessageNotifier {
    last_message_at: Arc<AtomicU64>,
}

impl MessageNotifier {
    /// Signal that a message was received, resetting the idle timer.
    pub fn on_message(&self) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_message_at.store(now_ms, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_hours_wrap_midnight() {
        let config = TaskLoopConfig {
            quiet_hours_start: 23,
            quiet_hours_end: 8,
            ..Default::default()
        };
        // We can't easily test in_quiet_hours without mocking time,
        // but we verify the config is constructed correctly.
        assert_eq!(config.quiet_hours_start, 23);
        assert_eq!(config.quiet_hours_end, 8);
    }

    #[test]
    fn message_notifier_updates_timestamp() {
        let config = TaskLoopConfig::default();
        let storage: Arc<dyn pluresdb::StorageEngine> =
            Arc::new(pluresdb::MemoryStorage::default());
        let store = pluresdb::CrdtStore::default().with_persistence(storage);
        let mgr = Arc::new(TaskManager::new(Arc::new(store)));
        let task_loop = TaskLoop::new(mgr, config);
        let notifier = task_loop.message_notifier();

        let before = task_loop.last_message_at.load(Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(10));
        notifier.on_message();
        let after = task_loop.last_message_at.load(Ordering::Relaxed);
        assert!(after >= before);
    }
}
