//! Subagent spawn actor for dev-lifecycle orchestration.
//!
//! Bridges between the reactive .px procedure system and the platform
//! sub-agent spawn seam. When `spawn_subagent` is called by a .px procedure,
//! this actor:
//!
//! 1. Spawns a session via a [`SubAgentSpawner`] (implemented by cognition's
//!    delegation `SubAgentManager`)
//! 2. Returns immediately with `{spawned: true, session_id: "..."}`
//! 3. On completion, writes to PluresDB key `stage_complete:{task_id}:{stage_name}`
//! 4. That PluresDB write triggers `evaluate_gate` via the reactive registry
//!
//! This creates the async feedback loop:
//! ```text
//! plan_task → spawn_subagent → (agent executes) → stage_complete write → evaluate_gate → spawn_subagent → ...
//! ```
//!
//! The actor depends only on platform types ([`crate::subagent_spawn`]); it has
//! no dependency on cognition's `delegation` module.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{debug, error, info, warn};

use crate::px_adapter::AsyncActionHandler;
use crate::spine::reactive::ReactiveRegistry;
use crate::subagent_spawn::{SessionStatus, SpawnOptions, SubAgentSpawner};
use pares_radix_praxis::px::executor::ExecutionError;

/// Actor that spawns subagent sessions and wires their completion back
/// into the reactive registry via PluresDB writes.
pub struct SubagentActor {
    /// Sub-agent spawner for spawning and tracking sessions (platform seam).
    manager: Arc<dyn SubAgentSpawner>,
    /// Reactive registry for triggering `evaluate_gate` on stage completion.
    registry: Arc<ReactiveRegistry>,
}

impl SubagentActor {
    /// Create a new subagent actor.
    ///
    /// # Arguments
    /// * `manager` — The sub-agent spawner that handles spawning
    /// * `registry` — The reactive registry for writing stage_complete events
    pub fn new(manager: Arc<dyn SubAgentSpawner>, registry: Arc<ReactiveRegistry>) -> Self {
        Self { manager, registry }
    }

    /// Spawn a subagent for a dev-lifecycle stage.
    ///
    /// Params:
    /// ```json
    /// {
    ///   "task_id": "TASK-2024-01-01-001",
    ///   "stage_name": "fix",
    ///   "prompt": "## Dev Lifecycle: fix stage\n...",
    ///   "workdir": "/projects/pares-radix",
    ///   "timeout_seconds": 600
    /// }
    /// ```
    ///
    /// Returns immediately with `{spawned: true, session_id: "..."}`.
    /// On completion, writes `stage_complete:{task_id}:{stage_name}` to trigger
    /// the reactive gate evaluation.
    async fn spawn_subagent(&self, params: &Value) -> Result<Value, ExecutionError> {
        let task_id = params
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "spawn_subagent".into(),
                message: "missing 'task_id'".into(),
            })?
            .to_string();

        let stage_name = params
            .get("stage_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "spawn_subagent".into(),
                message: "missing 'stage_name'".into(),
            })?
            .to_string();

        let prompt = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "spawn_subagent".into(),
                message: "missing 'prompt'".into(),
            })?
            .to_string();

        let timeout_seconds = params
            .get("timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(600);

        let _workdir = params
            .get("workdir")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        info!(
            task_id = %task_id,
            stage = %stage_name,
            timeout_s = timeout_seconds,
            "spawn_subagent: spawning stage execution"
        );

        // Configure spawn options
        let options = SpawnOptions::default()
            .with_timeout(Duration::from_secs(timeout_seconds))
            .with_label(format!("dev-lifecycle:{task_id}:{stage_name}"))
            .with_parent_context(format!(
                "Dev lifecycle stage execution. Task: {task_id}, Stage: {stage_name}"
            ));

        // Spawn the subagent — uses the "coder" agent for dev work
        let agent_name = match stage_name.as_str() {
            "analyze" => "analyst",
            "fix" | "test" | "deploy" => "coder",
            "verify" => "researcher",
            _ => "coder",
        };

        let session_id = self.manager.spawn(agent_name, &prompt, options).await;

        // Set up completion listener that writes to stage_complete
        let registry = Arc::clone(&self.registry);
        let tid = task_id.clone();
        let sname = stage_name.clone();
        let sid = session_id.clone();
        let mgr = Arc::clone(&self.manager);

        tokio::spawn(async move {
            // Poll for completion (the manager pushes events, but we need to
            // check the session status since we don't own the rx channel)
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if let Some(info) = mgr.get(&sid).await {
                    match &info.status {
                        SessionStatus::Completed => {
                            let output = info.output.unwrap_or_default();
                            info!(
                                task_id = %tid,
                                stage = %sname,
                                "subagent completed successfully"
                            );
                            // Write stage_complete to trigger evaluate_gate
                            let key = format!("stage_complete:{tid}:{sname}");
                            let value = json!({
                                "task_id": tid,
                                "stage_name": sname,
                                "status": "passed",
                                "output": output,
                                "attempts": 1
                            });
                            registry.on_write(&key, &value).await;
                            break;
                        }
                        SessionStatus::Failed(err) => {
                            warn!(
                                task_id = %tid,
                                stage = %sname,
                                error = %err,
                                "subagent failed"
                            );
                            let key = format!("stage_complete:{tid}:{sname}");
                            let value = json!({
                                "task_id": tid,
                                "stage_name": sname,
                                "status": "failed",
                                "output": err,
                                "attempts": 1
                            });
                            registry.on_write(&key, &value).await;
                            break;
                        }
                        SessionStatus::TimedOut => {
                            warn!(
                                task_id = %tid,
                                stage = %sname,
                                "subagent timed out"
                            );
                            let key = format!("stage_complete:{tid}:{sname}");
                            let value = json!({
                                "task_id": tid,
                                "stage_name": sname,
                                "status": "failed",
                                "output": "Stage execution timed out",
                                "attempts": 1
                            });
                            registry.on_write(&key, &value).await;
                            break;
                        }
                        SessionStatus::Killed => {
                            warn!(
                                task_id = %tid,
                                stage = %sname,
                                "subagent killed"
                            );
                            let key = format!("stage_complete:{tid}:{sname}");
                            let value = json!({
                                "task_id": tid,
                                "stage_name": sname,
                                "status": "blocked",
                                "output": "Stage execution was killed",
                                "attempts": 1
                            });
                            registry.on_write(&key, &value).await;
                            break;
                        }
                        SessionStatus::Running => {
                            // Still running, continue polling
                            continue;
                        }
                    }
                } else {
                    error!(
                        task_id = %tid,
                        stage = %sname,
                        session_id = %sid,
                        "subagent session not found — lost?"
                    );
                    break;
                }
            }
        });

        debug!(
            session_id = %session_id,
            task_id = %task_id,
            stage = %stage_name,
            "spawn_subagent: spawned successfully"
        );

        Ok(json!({
            "spawned": true,
            "session_id": session_id,
            "task_id": task_id,
            "stage_name": stage_name
        }))
    }
}

/// Actions handled by the subagent actor.
const SUBAGENT_ACTIONS: &[&str] = &["spawn_subagent"];

/// Check if an action name is handled by the subagent actor.
pub fn is_subagent_action(action: &str) -> bool {
    SUBAGENT_ACTIONS.contains(&action)
}

#[async_trait]
impl AsyncActionHandler for SubagentActor {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "spawn_subagent" => self.spawn_subagent(params).await,
            _ => Err(ExecutionError::ActionFailed {
                action: name.to_string(),
                message: "not a subagent action".into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subagent_spawn::{SessionStatus, SpawnedInfo};

    /// Minimal in-file spawner used to exercise the actor without any
    /// cognition (`delegation`) scaffolding.
    struct MockSpawner;

    #[async_trait]
    impl SubAgentSpawner for MockSpawner {
        async fn spawn(&self, _agent: &str, _prompt: &str, _options: SpawnOptions) -> String {
            "mock-session-1".to_string()
        }

        async fn get(&self, _session_id: &str) -> Option<SpawnedInfo> {
            Some(SpawnedInfo {
                status: SessionStatus::Completed,
                output: Some("Result: PASS".into()),
            })
        }
    }

    fn make_actor() -> SubagentActor {
        let manager: Arc<dyn SubAgentSpawner> = Arc::new(MockSpawner);
        let registry = Arc::new(ReactiveRegistry::new());
        SubagentActor::new(manager, registry)
    }

    #[tokio::test]
    async fn spawn_subagent_returns_immediately() {
        let actor = make_actor();
        let params = json!({
            "task_id": "TASK-001",
            "stage_name": "fix",
            "prompt": "Fix the bug",
            "workdir": "/tmp",
            "timeout_seconds": 60
        });

        let result = actor.call("spawn_subagent", &params).await.unwrap();
        assert_eq!(result["spawned"], true);
        assert!(result["session_id"].is_string());
        assert_eq!(result["task_id"], "TASK-001");
        assert_eq!(result["stage_name"], "fix");
    }

    #[tokio::test]
    async fn spawn_subagent_missing_task_id_errors() {
        let actor = make_actor();
        let params = json!({
            "stage_name": "fix",
            "prompt": "Fix the bug"
        });

        let result = actor.call("spawn_subagent", &params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn unknown_action_errors() {
        let actor = make_actor();
        let result = actor.call("unknown_action", &json!({})).await;
        assert!(result.is_err());
    }
}
