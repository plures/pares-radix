//! `scheduler` — tokio-based task scheduler for pares-radix.
//!
//! Provides cron-expression and interval-based task scheduling, with
//! tasks persisted in PluresDB so schedules survive process restarts.
//!
//! # Example
//! ```rust,ignore
//! use pares_agens_agenda::scheduler::{Scheduler, Task, Schedule};
//! let scheduler = Scheduler::new();
//! // scheduler.add(task).await;
//! // scheduler.start().await;
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Timelike, Utc};
use pluresdb::{CrdtStore, MemoryStorage, SledStorage, StorageEngine};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::time::{self, Duration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// A scheduled task definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// When to run.
    pub schedule: Schedule,
    /// Command to execute (passed to the agent's run_command tool).
    pub command: String,
    /// Whether the task is active.
    pub enabled: bool,
    /// Last execution time.
    #[serde(default)]
    pub last_run: Option<DateTime<Utc>>,
    /// Last execution result (truncated).
    #[serde(default)]
    pub last_result: Option<String>,
}

/// Schedule definition — when a task should fire.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Schedule {
    /// Run at a fixed interval.
    #[serde(rename = "interval")]
    Interval {
        /// Interval in seconds.
        every_secs: u64,
    },
    /// Run on a cron expression (minute hour day month weekday).
    #[serde(rename = "cron")]
    Cron {
        /// Cron expression (5-field: min hour dom month dow).
        expr: String,
    },
    /// Run once at a specific time.
    #[serde(rename = "once")]
    Once {
        /// ISO 8601 timestamp.
        at: DateTime<Utc>,
    },
}

/// Callback type for task execution.
pub type TaskExecutor = Arc<dyn Fn(String) -> tokio::task::JoinHandle<String> + Send + Sync>;

const TASK_PREFIX: &str = "agenda/task/";
const TASK_ACTOR: &str = "plures-agenda";

/// Errors produced by scheduler task persistence backends.
#[derive(Debug, Error)]
pub enum SchedulerStoreError {
    /// The underlying store failed.
    #[error("store error: {0}")]
    Store(String),
    /// Task serialization/deserialization failed.
    #[error("serialisation error: {0}")]
    Serialise(String),
}

/// Persistence backend for scheduler tasks.
#[async_trait]
pub trait TaskStore: Send + Sync {
    /// Insert or overwrite a task by ID.
    async fn upsert(&self, task: Task) -> Result<(), SchedulerStoreError>;
    /// Delete a task by ID.
    async fn delete(&self, id: &str) -> Result<(), SchedulerStoreError>;
    /// Return all persisted tasks.
    async fn all(&self) -> Result<Vec<Task>, SchedulerStoreError>;
}

/// PluresDB-backed task store.
pub struct PluresDbTaskStore {
    store: Arc<CrdtStore>,
}

impl PluresDbTaskStore {
    /// Open or create a durable PluresDB-backed scheduler task store.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerStoreError::Store`] when PluresDB cannot be opened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SchedulerStoreError> {
        let storage: Arc<dyn StorageEngine> = Arc::new(
            SledStorage::open(path)
                .map_err(|e| SchedulerStoreError::Store(format!("open failed: {e}")))?,
        );
        let store = CrdtStore::default().with_persistence(storage);
        Ok(Self {
            store: Arc::new(store),
        })
    }

    /// Create an ephemeral in-memory PluresDB-backed task store.
    #[must_use]
    pub fn in_memory() -> Self {
        let storage: Arc<dyn StorageEngine> = Arc::new(MemoryStorage::default());
        let store = CrdtStore::default().with_persistence(storage);
        Self {
            store: Arc::new(store),
        }
    }
}

#[async_trait]
impl TaskStore for PluresDbTaskStore {
    async fn upsert(&self, task: Task) -> Result<(), SchedulerStoreError> {
        let key = format!("{TASK_PREFIX}{}", task.id);
        let value = serde_json::to_value(task)
            .map_err(|e| SchedulerStoreError::Serialise(format!("encode task failed: {e}")))?;
        self.store.put(key, TASK_ACTOR, value);
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), SchedulerStoreError> {
        let key = format!("{TASK_PREFIX}{id}");
        match self.store.delete(&key) {
            Ok(()) => Ok(()),
            Err(_) => Ok(()),
        }
    }

    async fn all(&self) -> Result<Vec<Task>, SchedulerStoreError> {
        let mut tasks = Vec::new();
        for record in self
            .store
            .list()
            .into_iter()
            .filter(|record| record.id.starts_with(TASK_PREFIX))
        {
            match serde_json::from_value::<Task>(record.data) {
                Ok(task) => tasks.push(task),
                Err(e) => {
                    warn!(record_id = %record.id, error = %e, "skipping invalid persisted task record");
                }
            }
        }
        Ok(tasks)
    }
}

/// Errors produced by scheduler slash-command parsing.
#[derive(Debug, Error, PartialEq)]
pub enum SchedulerCommandError {
    /// The command is malformed.
    #[error("invalid /cron command syntax")]
    InvalidSyntax,
    /// The schedule expression is malformed.
    #[error("invalid schedule expression: {0}")]
    InvalidSchedule(String),
    /// The command text is empty.
    #[error("command text must not be empty")]
    EmptyCommand,
}

/// The scheduler — manages and executes scheduled tasks.
pub struct Scheduler {
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    executor: Option<TaskExecutor>,
    store: Option<Arc<dyn TaskStore>>,
}

impl Scheduler {
    /// Create a new empty scheduler.
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            executor: None,
            store: None,
        }
    }

    /// Set the task executor callback.
    ///
    /// The executor receives the task's `command` string and should return
    /// a JoinHandle that resolves to the command output.
    pub fn with_executor(mut self, executor: TaskExecutor) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Configure persistent task storage.
    pub fn with_store(mut self, store: Arc<dyn TaskStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Load all persisted tasks from the configured store into memory.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerStoreError`] when task loading fails.
    pub async fn load_persisted(&self) -> Result<usize, SchedulerStoreError> {
        let Some(store) = &self.store else {
            return Ok(0);
        };
        let tasks = store.all().await?;
        let loaded = tasks.len();
        let mut guard = self.tasks.write().await;
        for task in tasks {
            guard.insert(task.id.clone(), task);
        }
        Ok(loaded)
    }

    /// Add or update a task.
    pub async fn add(&self, task: Task) {
        info!(id = %task.id, name = %task.name, "scheduled task added");
        let task_id = task.id.clone();
        self.tasks
            .write()
            .await
            .insert(task.id.clone(), task.clone());
        if let Some(store) = &self.store {
            if let Err(e) = store.upsert(task).await {
                error!(task = %task_id, error = %e, "failed to persist scheduled task");
            }
        }
    }

    /// Remove a task by ID.
    pub async fn remove(&self, id: &str) -> bool {
        let existed = self.tasks.write().await.remove(id).is_some();
        if let Some(store) = &self.store {
            if let Err(e) = store.delete(id).await {
                error!(task = %id, error = %e, "failed to delete persisted scheduled task");
            }
        }
        existed
    }

    /// List all tasks.
    pub async fn list(&self) -> Vec<Task> {
        self.tasks.read().await.values().cloned().collect()
    }

    /// Get a task by ID.
    pub async fn get(&self, id: &str) -> Option<Task> {
        self.tasks.read().await.get(id).cloned()
    }

    /// Enable or disable a task.
    pub async fn set_enabled(&self, id: &str, enabled: bool) -> bool {
        let maybe_task = if let Some(task) = self.tasks.write().await.get_mut(id) {
            task.enabled = enabled;
            Some(task.clone())
        } else {
            None
        };
        if let Some(task) = maybe_task {
            if let Some(store) = &self.store {
                if let Err(e) = store.upsert(task).await {
                    error!(task = %id, error = %e, "failed to persist enabled state");
                }
            }
            true
        } else {
            false
        }
    }

    /// Start the scheduler loop. Runs until the Scheduler is dropped.
    ///
    /// Checks every 10 seconds for tasks that are due to run.
    pub async fn start(&self) {
        let tasks = Arc::clone(&self.tasks);
        let executor = self.executor.clone();
        let store = self.store.clone();

        info!("Scheduler started — checking every 10s");

        let mut interval = time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;

            let now = Utc::now();
            let mut due_tasks = Vec::new();

            {
                let guard = tasks.read().await;
                for task in guard.values() {
                    if !task.enabled {
                        continue;
                    }
                    if Self::is_due(task, &now) {
                        due_tasks.push(task.clone());
                    }
                }
            }

            for task in due_tasks {
                debug!(id = %task.id, name = %task.name, "task is due");

                if let Some(ref executor) = executor {
                    let cmd = task.command.clone();
                    let task_id = task.id.clone();
                    let tasks_ref = Arc::clone(&tasks);
                    let exec = Arc::clone(executor);
                    let task_store = store.clone();

                    tokio::spawn(async move {
                        let handle = exec(cmd);
                        match handle.await {
                            Ok(result) => {
                                let truncated = if result.len() > 500 {
                                    format!("{}...", &result[..500])
                                } else {
                                    result
                                };
                                info!(task = %task_id, "task completed");
                                let mut persisted_task = None;
                                if let Some(t) = tasks_ref.write().await.get_mut(&task_id) {
                                    t.last_run = Some(Utc::now());
                                    t.last_result = Some(truncated);
                                    persisted_task = Some(t.clone());
                                }
                                if let (Some(task), Some(store)) =
                                    (persisted_task, task_store.clone())
                                {
                                    if let Err(e) = store.upsert(task).await {
                                        error!(task = %task_id, error = %e, "failed to persist task completion");
                                    }
                                }
                            }
                            Err(e) => {
                                error!(task = %task_id, error = %e, "task execution failed");
                                let mut persisted_task = None;
                                if let Some(t) = tasks_ref.write().await.get_mut(&task_id) {
                                    t.last_run = Some(Utc::now());
                                    t.last_result = Some(format!("ERROR: {e}"));
                                    persisted_task = Some(t.clone());
                                }
                                if let (Some(task), Some(store)) =
                                    (persisted_task, task_store.clone())
                                {
                                    if let Err(e) = store.upsert(task).await {
                                        error!(task = %task_id, error = %e, "failed to persist task error state");
                                    }
                                }
                            }
                        }
                    });
                } else {
                    warn!(task = %task.id, "no executor configured — skipping");
                }

                // Mark as run to prevent re-firing within the same tick
                if let Some(t) = tasks.write().await.get_mut(&task.id) {
                    t.last_run = Some(now);
                }
            }
        }
    }

    /// Check if a task is due to run now.
    fn is_due(task: &Task, now: &DateTime<Utc>) -> bool {
        match &task.schedule {
            Schedule::Interval { every_secs } => {
                let interval = chrono::Duration::seconds(*every_secs as i64);
                match &task.last_run {
                    Some(last) => *now - *last >= interval,
                    None => true, // never run → due immediately
                }
            }
            Schedule::Once { at } => task.last_run.is_none() && *now >= *at,
            Schedule::Cron { expr } => {
                let parts: Vec<&str> = expr.split_whitespace().collect();
                if parts.len() != 5 {
                    return false;
                }

                let minute = now.minute();
                let hour = now.hour();
                let day = now.day();
                let month = now.month();
                let weekday = now.weekday().num_days_from_sunday();

                let min_match = Self::matches_cron_field(parts[0], minute, 0, 59);
                let hour_match = Self::matches_cron_field(parts[1], hour, 0, 23);
                let day_match = Self::matches_cron_field(parts[2], day, 1, 31);
                let month_match = Self::matches_cron_field(parts[3], month, 1, 12);
                let weekday_match = Self::matches_cron_field(parts[4], weekday, 0, 6);

                // Only fire once per minute (check last_run)
                let not_already_run = match &task.last_run {
                    Some(last) => (*now - *last).num_seconds() >= 60,
                    None => true,
                };

                min_match
                    && hour_match
                    && day_match
                    && month_match
                    && weekday_match
                    && not_already_run
            }
        }
    }

    fn matches_cron_field(field: &str, value: u32, min: u32, max: u32) -> bool {
        if field == "*" {
            return true;
        }

        field
            .split(',')
            .any(|part| Self::matches_cron_part(part.trim(), value, min, max))
    }

    fn matches_cron_part(part: &str, value: u32, min: u32, max: u32) -> bool {
        if part.is_empty() {
            return false;
        }

        let (base, step) = if let Some((lhs, rhs)) = part.split_once('/') {
            let parsed_step = rhs.parse::<u32>().ok().filter(|step| *step > 0);
            if parsed_step.is_none() {
                return false;
            }
            (
                lhs,
                parsed_step.expect("parsed_step validated as Some and greater than zero"),
            )
        } else {
            (part, 1)
        };

        let in_base = if base == "*" {
            value >= min && value <= max
        } else if let Some((start, end)) = base.split_once('-') {
            let start = start.parse::<u32>().ok();
            let end = end.parse::<u32>().ok();
            matches!(
                (start, end),
                (Some(start), Some(end))
                    if start >= min && end <= max && start <= end && value >= start && value <= end
            )
        } else {
            base.parse::<u32>()
                .map(|v| v >= min && v <= max && v == value)
                .unwrap_or(false)
        };

        if !in_base {
            return false;
        }

        if step == 1 {
            true
        } else if base == "*" {
            (value - min).is_multiple_of(step)
        } else if let Some((start, _)) = base.split_once('-') {
            start
                .parse::<u32>()
                .ok()
                .map(|start| (value - start).is_multiple_of(step))
                .unwrap_or(false)
        } else {
            true
        }
    }

    /// Parse `/cron add '<schedule>' '<command>'` into a task.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerCommandError`] when the command is malformed.
    pub fn parse_cron_add(command_text: &str) -> Result<Task, SchedulerCommandError> {
        let args = Self::tokenize_quoted(command_text)?;
        if args.len() != 4 || args[0] != "/cron" || args[1] != "add" {
            return Err(SchedulerCommandError::InvalidSyntax);
        }

        let schedule_raw = args[2].trim();
        let command = args[3].trim();
        if command.is_empty() {
            return Err(SchedulerCommandError::EmptyCommand);
        }

        let schedule = if let Some(rest) = schedule_raw.strip_prefix("every ") {
            let secs = Self::parse_interval_secs(rest.trim())
                .ok_or_else(|| SchedulerCommandError::InvalidSchedule(schedule_raw.to_string()))?;
            Schedule::Interval { every_secs: secs }
        } else {
            let parts: Vec<&str> = schedule_raw.split_whitespace().collect();
            if parts.len() != 5 {
                return Err(SchedulerCommandError::InvalidSchedule(
                    schedule_raw.to_string(),
                ));
            }
            Schedule::Cron {
                expr: schedule_raw.to_string(),
            }
        };

        Ok(Task {
            id: format!("cron.{}", Uuid::new_v4()),
            name: command.to_string(),
            schedule,
            command: command.to_string(),
            enabled: true,
            last_run: None,
            last_result: None,
        })
    }

    fn tokenize_quoted(input: &str) -> Result<Vec<String>, SchedulerCommandError> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut quote: Option<char> = None;

        for ch in input.chars() {
            match quote {
                Some(active) if ch == active => {
                    quote = None;
                }
                Some(_) => current.push(ch),
                None if ch == '\'' || ch == '"' => {
                    quote = Some(ch);
                }
                None if ch.is_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                None => current.push(ch),
            }
        }

        if quote.is_some() {
            return Err(SchedulerCommandError::InvalidSyntax);
        }
        if !current.is_empty() {
            tokens.push(current);
        }
        Ok(tokens)
    }

    fn parse_interval_secs(raw: &str) -> Option<u64> {
        let split_at = raw
            .char_indices()
            .find_map(|(idx, ch)| (!ch.is_ascii_digit()).then_some(idx))
            .unwrap_or(raw.len());
        let (value, unit) = raw.split_at(split_at);
        let amount = value.parse::<u64>().ok()?;
        if amount == 0 {
            return None;
        }
        match unit {
            "s" => Some(amount),
            "m" => amount.checked_mul(60),
            "h" => amount.checked_mul(60)?.checked_mul(60),
            _ => None,
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn interval_due_when_never_run() {
        let task = Task {
            id: "t1".into(),
            name: "test".into(),
            schedule: Schedule::Interval { every_secs: 60 },
            command: "echo hi".into(),
            enabled: true,
            last_run: None,
            last_result: None,
        };
        assert!(Scheduler::is_due(&task, &Utc::now()));
    }

    #[test]
    fn interval_not_due_when_recent() {
        let task = Task {
            id: "t1".into(),
            name: "test".into(),
            schedule: Schedule::Interval { every_secs: 60 },
            command: "echo hi".into(),
            enabled: true,
            last_run: Some(Utc::now()),
            last_result: None,
        };
        assert!(!Scheduler::is_due(&task, &Utc::now()));
    }

    #[test]
    fn once_due_when_past() {
        let past = Utc::now() - chrono::Duration::hours(1);
        let task = Task {
            id: "t1".into(),
            name: "test".into(),
            schedule: Schedule::Once { at: past },
            command: "echo hi".into(),
            enabled: true,
            last_run: None,
            last_result: None,
        };
        assert!(Scheduler::is_due(&task, &Utc::now()));
    }

    #[test]
    fn once_not_due_after_run() {
        let past = Utc::now() - chrono::Duration::hours(1);
        let task = Task {
            id: "t1".into(),
            name: "test".into(),
            schedule: Schedule::Once { at: past },
            command: "echo hi".into(),
            enabled: true,
            last_run: Some(Utc::now()),
            last_result: None,
        };
        assert!(!Scheduler::is_due(&task, &Utc::now()));
    }

    #[test]
    fn disabled_task_never_due() {
        let task = Task {
            id: "t1".into(),
            name: "test".into(),
            schedule: Schedule::Interval { every_secs: 1 },
            command: "echo hi".into(),
            enabled: false,
            last_run: None,
            last_result: None,
        };
        // is_due doesn't check enabled — caller does
        assert!(Scheduler::is_due(&task, &Utc::now()));
    }

    #[test]
    fn cron_matches_all_fields() {
        let now = Utc
            .with_ymd_and_hms(2026, 4, 20, 12, 30, 0)
            .single()
            .unwrap();
        let task = Task {
            id: "cron1".into(),
            name: "cron".into(),
            schedule: Schedule::Cron {
                expr: "30 12 20 4 1".into(),
            },
            command: "echo ok".into(),
            enabled: true,
            last_run: None,
            last_result: None,
        };
        assert!(Scheduler::is_due(&task, &now));
    }

    #[test]
    fn cron_rejects_non_matching_day() {
        let now = Utc
            .with_ymd_and_hms(2026, 4, 20, 12, 30, 0)
            .single()
            .unwrap();
        let task = Task {
            id: "cron2".into(),
            name: "cron".into(),
            schedule: Schedule::Cron {
                expr: "30 12 21 4 1".into(),
            },
            command: "echo ok".into(),
            enabled: true,
            last_run: None,
            last_result: None,
        };
        assert!(!Scheduler::is_due(&task, &now));
    }

    #[test]
    fn parse_cron_add_with_expression() {
        let task = Scheduler::parse_cron_add("/cron add '*/15 * * * *' 'check org CI'").unwrap();
        assert_eq!(task.name, "check org CI");
        assert_eq!(task.command, "check org CI");
        assert!(matches!(task.schedule, Schedule::Cron { .. }));
        assert!(task.enabled);
    }

    #[test]
    fn parse_cron_add_with_interval() {
        let task = Scheduler::parse_cron_add("/cron add 'every 30s' 'stale pr check'").unwrap();
        assert!(matches!(
            task.schedule,
            Schedule::Interval { every_secs: 30 }
        ));
    }

    // ── Mutation gap coverage tests ─────────────────────────────────────────────

    #[test]
    fn parse_interval_minutes() {
        assert_eq!(Scheduler::parse_interval_secs("5m"), Some(300));
        assert_eq!(Scheduler::parse_interval_secs("1m"), Some(60));
    }

    #[test]
    fn parse_interval_hours() {
        assert_eq!(Scheduler::parse_interval_secs("2h"), Some(7200));
        assert_eq!(Scheduler::parse_interval_secs("1h"), Some(3600));
    }

    #[test]
    fn parse_interval_invalid_unit() {
        assert_eq!(Scheduler::parse_interval_secs("5d"), None);
        assert_eq!(Scheduler::parse_interval_secs("0s"), None);
    }

    #[test]
    fn cron_part_exact_value() {
        // Tests exact match: value == parsed
        assert!(Scheduler::matches_cron_part("5", 5, 0, 59));
        assert!(!Scheduler::matches_cron_part("5", 6, 0, 59));
        assert!(!Scheduler::matches_cron_part("5", 4, 0, 59));
    }

    #[test]
    fn cron_part_range() {
        // Tests range: start <= value <= end
        assert!(Scheduler::matches_cron_part("5-10", 5, 0, 59));
        assert!(Scheduler::matches_cron_part("5-10", 10, 0, 59));
        assert!(Scheduler::matches_cron_part("5-10", 7, 0, 59));
        assert!(!Scheduler::matches_cron_part("5-10", 4, 0, 59));
        assert!(!Scheduler::matches_cron_part("5-10", 11, 0, 59));
    }

    #[test]
    fn cron_part_step_on_star() {
        // */5 at min=0: 0, 5, 10, 15... match; 1, 2, 3, 4 don't
        assert!(Scheduler::matches_cron_part("*/5", 0, 0, 59));
        assert!(Scheduler::matches_cron_part("*/5", 5, 0, 59));
        assert!(Scheduler::matches_cron_part("*/5", 10, 0, 59));
        assert!(!Scheduler::matches_cron_part("*/5", 1, 0, 59));
        assert!(!Scheduler::matches_cron_part("*/5", 3, 0, 59));
    }

    #[test]
    fn cron_part_step_on_range() {
        // 2-10/3: values 2, 5, 8 match; 3, 4, 6, 7, 9, 10 don't
        assert!(Scheduler::matches_cron_part("2-10/3", 2, 0, 59));
        assert!(Scheduler::matches_cron_part("2-10/3", 5, 0, 59));
        assert!(Scheduler::matches_cron_part("2-10/3", 8, 0, 59));
        assert!(!Scheduler::matches_cron_part("2-10/3", 3, 0, 59));
        assert!(!Scheduler::matches_cron_part("2-10/3", 4, 0, 59));
        assert!(!Scheduler::matches_cron_part("2-10/3", 10, 0, 59));
    }

    #[test]
    fn cron_part_invalid_step() {
        assert!(!Scheduler::matches_cron_part("*/0", 5, 0, 59));
        assert!(!Scheduler::matches_cron_part("5-10/abc", 7, 0, 59));
    }

    #[test]
    fn cron_part_out_of_bounds() {
        // value > max
        assert!(!Scheduler::matches_cron_part("*", 60, 0, 59));
        // value < min
        assert!(!Scheduler::matches_cron_part("*", 0, 1, 31));
    }

    #[test]
    fn cron_part_step_star_with_nonzero_min() {
        // */3 on day field (min=1): day 1,4,7,10... match
        // (value - min) % step == 0 → (4-1)=3, 3%3==0 ✓
        assert!(Scheduler::matches_cron_part("*/3", 4, 1, 31));
        assert!(Scheduler::matches_cron_part("*/3", 7, 1, 31));
        assert!(Scheduler::matches_cron_part("*/3", 1, 1, 31));
        // (2-1)=1, 1%3≠0
        assert!(!Scheduler::matches_cron_part("*/3", 2, 1, 31));
        // (3-1)=2, 2%3≠0
        assert!(!Scheduler::matches_cron_part("*/3", 3, 1, 31));
    }

    #[test]
    fn cron_part_step_zero_invalid() {
        // step=0 should be rejected even though u32 >= 0
        assert!(!Scheduler::matches_cron_part("*/0", 0, 0, 59));
        assert!(!Scheduler::matches_cron_part("5-10/0", 5, 0, 59));
    }

    #[test]
    fn parse_cron_add_wrong_verb() {
        // Tests that args[1] != "add" is properly checked (|| not &&)
        let result = Scheduler::parse_cron_add("/cron list '* * * * *' 'test'");
        assert!(result.is_err());
    }

    #[test]
    fn parse_cron_add_wrong_prefix() {
        // Tests that args[0] != "/cron" is properly checked
        let result = Scheduler::parse_cron_add("/sched add '* * * * *' 'test'");
        assert!(result.is_err());
    }

    #[test]
    fn parse_cron_add_invalid_empty_command() {
        let result = Scheduler::parse_cron_add("/cron add '* * * * *' ''");
        assert!(result.is_err());
    }

    #[test]
    fn parse_cron_add_invalid_syntax() {
        // Only 3 args instead of 4
        let result = Scheduler::parse_cron_add("/cron add '* * * * *'");
        assert!(result.is_err());
    }

    #[test]
    fn parse_cron_add_bad_cron_field_count() {
        // 4 fields instead of 5
        let result = Scheduler::parse_cron_add("/cron add '* * * *' 'test'");
        assert!(result.is_err());
    }

    #[test]
    fn parse_cron_add_interval_minutes() {
        let task = Scheduler::parse_cron_add("/cron add 'every 5m' 'check'").unwrap();
        assert!(matches!(
            task.schedule,
            Schedule::Interval { every_secs: 300 }
        ));
    }

    #[test]
    fn parse_cron_add_interval_hours() {
        let task = Scheduler::parse_cron_add("/cron add 'every 2h' 'check'").unwrap();
        assert!(matches!(
            task.schedule,
            Schedule::Interval { every_secs: 7200 }
        ));
    }

    #[tokio::test]
    async fn remove_existing_task() {
        let scheduler = Scheduler::new();
        scheduler
            .add(Task {
                id: "r1".into(),
                name: "removable".into(),
                schedule: Schedule::Interval { every_secs: 60 },
                command: "echo".into(),
                enabled: true,
                last_run: None,
                last_result: None,
            })
            .await;
        assert!(scheduler.remove("r1").await);
        assert!(!scheduler.remove("r1").await);
    }

    #[tokio::test]
    async fn get_existing_and_missing() {
        let scheduler = Scheduler::new();
        scheduler
            .add(Task {
                id: "g1".into(),
                name: "getme".into(),
                schedule: Schedule::Interval { every_secs: 60 },
                command: "echo".into(),
                enabled: true,
                last_run: None,
                last_result: None,
            })
            .await;
        assert!(scheduler.get("g1").await.is_some());
        assert!(scheduler.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn set_enabled_existing_and_missing() {
        let scheduler = Scheduler::new();
        scheduler
            .add(Task {
                id: "e1".into(),
                name: "toggle".into(),
                schedule: Schedule::Interval { every_secs: 60 },
                command: "echo".into(),
                enabled: true,
                last_run: None,
                last_result: None,
            })
            .await;
        assert!(scheduler.set_enabled("e1", false).await);
        let t = scheduler.get("e1").await.unwrap();
        assert!(!t.enabled);
        assert!(!scheduler.set_enabled("nonexist", true).await);
    }

    #[tokio::test]
    async fn cron_not_due_if_ran_within_60s() {
        let now = Utc
            .with_ymd_and_hms(2026, 4, 20, 12, 30, 0)
            .single()
            .unwrap();
        let task = Task {
            id: "cron_dup".into(),
            name: "cron".into(),
            schedule: Schedule::Cron {
                expr: "30 12 20 4 1".into(),
            },
            command: "echo ok".into(),
            enabled: true,
            last_run: Some(now - chrono::Duration::seconds(30)),
            last_result: None,
        };
        assert!(!Scheduler::is_due(&task, &now));
    }

    #[tokio::test]
    async fn with_executor_stores_executor() {
        let executor: TaskExecutor =
            Arc::new(|_cmd: String| tokio::spawn(async { "done".to_string() }));
        let scheduler = Scheduler::new().with_executor(executor);
        assert!(scheduler.executor.is_some());
    }

    #[tokio::test]
    async fn delete_persisted_task_removes_from_store() {
        let store = Arc::new(PluresDbTaskStore::in_memory());
        let scheduler = Scheduler::new().with_store(store.clone());
        scheduler
            .add(Task {
                id: "del1".into(),
                name: "deletable".into(),
                schedule: Schedule::Interval { every_secs: 60 },
                command: "echo".into(),
                enabled: true,
                last_run: None,
                last_result: None,
            })
            .await;
        scheduler.remove("del1").await;
        // Verify store doesn't have it
        let fresh = Scheduler::new().with_store(store);
        let loaded = fresh.load_persisted().await.unwrap();
        assert_eq!(loaded, 0);
    }

    #[tokio::test]
    async fn persisted_tasks_round_trip_through_pluresdb_store() {
        let store = Arc::new(PluresDbTaskStore::in_memory());
        let scheduler = Scheduler::new().with_store(store.clone());
        scheduler
            .add(Task {
                id: "persisted.task".into(),
                name: "persisted".into(),
                schedule: Schedule::Interval { every_secs: 60 },
                command: "echo persisted".into(),
                enabled: true,
                last_run: None,
                last_result: None,
            })
            .await;

        let fresh_scheduler = Scheduler::new().with_store(store);
        let loaded = fresh_scheduler.load_persisted().await.unwrap();
        assert_eq!(loaded, 1);

        let tasks = fresh_scheduler.list().await;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "persisted.task");
    }
}
