//! Durable, channel-neutral task custody transfer with PluresDB-backed CAS.
//!
//! Uses PluresDB (`CrdtStore` + `SledStorage`) for durable persistence and a
//! process-local `Mutex` to serialise the read-check-write sequence, giving
//! compare-and-swap-equivalent guarantees within a single process.

use pluresdb::{CrdtStore, SledStorage, StorageEngine};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use uuid::Uuid;

const ACTOR: &str = "radix-task-custody-v1";
const SCHEMA: &str = "plures.task-handoff.v1";
const TASK_PREFIX: &str = "task-custody:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CustodyState {
    Owned,
    TransferPending,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Open,
    InProgress,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

impl ExecutionState {
    fn terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferableTask {
    pub task_id: String,
    pub objective: String,
    pub repo: String,
    pub priority: String,
    pub constraints: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub next_action: String,
    pub provenance: String,
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustodyRecord {
    pub task: TransferableTask,
    pub owner_agent_id: String,
    pub target_agent_id: Option<String>,
    pub handoff_id: Option<Uuid>,
    pub handoff_generation: u64,
    pub custody_state: CustodyState,
    pub execution_state: ExecutionState,
    pub claimed_by_worker: Option<String>,
    pub claim_token: Option<Uuid>,
    pub blocked_reason: Option<String>,
    pub result: Option<String>,
    pub source_revision: String,
    pub content_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandoffEnvelope {
    pub schema: String,
    pub record: CustodyRecord,
    pub digest: String,
}

impl HandoffEnvelope {
    pub fn canonical_json(&self) -> Result<Vec<u8>, HandoffError> {
        Ok(serde_json::to_vec(self)?)
    }

    pub fn verify(&self) -> Result<(), HandoffError> {
        if self.schema != SCHEMA {
            return Err(HandoffError::InvalidEnvelope("unsupported schema".into()));
        }
        let expected = digest_record(&self.record)?;
        if expected != self.digest || self.record.content_digest.as_deref() != Some(&self.digest) {
            return Err(HandoffError::IntegrityConflict);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claim {
    pub task_id: String,
    pub worker_id: String,
    pub token: Uuid,
    pub generation: u64,
}

#[derive(Debug, Error)]
pub enum HandoffError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("task not found: {0}")]
    NotFound(String),
    #[error("conditional write conflict")]
    Conflict,
    #[error("task already claimed by another worker")]
    AlreadyClaimed,
    #[error("claim token does not authorize this update")]
    InvalidClaimToken,
    #[error("handoff integrity conflict")]
    IntegrityConflict,
    #[error("invalid handoff envelope: {0}")]
    InvalidEnvelope(String),
    #[error("invalid custody transition: {0}")]
    InvalidTransition(String),
}

/// A durable conditional store backed by PluresDB (`CrdtStore` + `SledStorage`).
///
/// Provides compare-and-swap semantics via a process-local `Mutex`: the
/// read-check-write triple is executed under the lock, giving CAS-equivalent
/// guarantees within a single process. `SledStorage` ensures data survives
/// process restarts.
#[derive(Clone)]
pub struct ConditionalTaskStore {
    crdt: Arc<CrdtStore>,
    cas_lock: Arc<Mutex<()>>,
}

impl ConditionalTaskStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, HandoffError> {
        let storage: Arc<dyn StorageEngine> = Arc::new(
            SledStorage::open(path.as_ref())
                .map_err(|e| HandoffError::Storage(format!("{e}")))?,
        );
        let crdt = Arc::new(CrdtStore::default().with_persistence(storage));
        Ok(Self {
            crdt,
            cas_lock: Arc::new(Mutex::new(())),
        })
    }

    fn node_id(task_id: &str) -> String {
        format!("{TASK_PREFIX}{task_id}")
    }

    fn read_record(&self, task_id: &str) -> Result<Option<CustodyRecord>, HandoffError> {
        match self.crdt.get(Self::node_id(task_id)) {
            None => Ok(None),
            Some(r) if r.data.is_null() => Ok(None),
            Some(r) => Ok(Some(serde_json::from_value(r.data)?)),
        }
    }

    fn write_record(&self, record: &CustodyRecord) -> Result<(), HandoffError> {
        let value = serde_json::to_value(record)?;
        self.crdt.put(Self::node_id(&record.task.task_id), ACTOR, value);
        Ok(())
    }

    /// Execute a compare-and-swap under the process-local lock.
    ///
    /// `expected = None` means "no existing record" (insert-only).
    /// Returns `true` if `next` was written, `false` on mismatch.
    fn cas(
        &self,
        task_id: &str,
        expected: Option<&CustodyRecord>,
        next: &CustodyRecord,
    ) -> Result<bool, HandoffError> {
        let _guard = self.cas_lock.lock().expect("cas_lock poisoned");
        let current = self.read_record(task_id)?;
        let matches = match (expected, current.as_ref()) {
            (None, None) => true,
            (Some(exp), Some(cur)) => exp == cur,
            _ => false,
        };
        if matches {
            self.write_record(next)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn inspect(&self, task_id: &str) -> Result<Option<CustodyRecord>, HandoffError> {
        self.read_record(task_id)
    }

    pub fn seed_owned(
        &self,
        task: TransferableTask,
        owner: &str,
    ) -> Result<CustodyRecord, HandoffError> {
        validate_nonempty("task_id", &task.task_id)?;
        validate_nonempty("owner", owner)?;
        let record = CustodyRecord {
            source_revision: digest_task(&task)?,
            task,
            owner_agent_id: owner.to_owned(),
            target_agent_id: None,
            handoff_id: None,
            handoff_generation: 0,
            custody_state: CustodyState::Owned,
            execution_state: ExecutionState::Open,
            claimed_by_worker: None,
            claim_token: None,
            blocked_reason: None,
            result: None,
            content_digest: None,
        };
        if self.cas(&record.task.task_id, None, &record)?
            || self.inspect(&record.task.task_id)?.as_ref() == Some(&record)
        {
            Ok(record)
        } else {
            Err(HandoffError::Conflict)
        }
    }

    pub fn prepare_handoff(
        &self,
        task_id: &str,
        source: &str,
        target: &str,
        handoff_id: Uuid,
        expected_generation: u64,
    ) -> Result<HandoffEnvelope, HandoffError> {
        validate_nonempty("source", source)?;
        validate_nonempty("target", target)?;
        if source == target {
            return Err(HandoffError::InvalidTransition(
                "source equals target".into(),
            ));
        }
        loop {
            let record = self
                .read_record(task_id)?
                .ok_or_else(|| HandoffError::NotFound(task_id.into()))?;
            if record.custody_state == CustodyState::TransferPending {
                if record.owner_agent_id == source
                    && record.target_agent_id.as_deref() == Some(target)
                    && record.handoff_id == Some(handoff_id)
                    && record.handoff_generation == expected_generation + 1
                {
                    return envelope(record);
                }
                return Err(HandoffError::Conflict);
            }
            if record.execution_state.terminal()
                || record.execution_state != ExecutionState::Open
                || record.owner_agent_id != source
                || record.handoff_generation != expected_generation
            {
                return Err(HandoffError::InvalidTransition(
                    "source, generation, or execution state mismatch".into(),
                ));
            }
            let expected = record.clone();
            let mut next = record;
            next.custody_state = CustodyState::TransferPending;
            next.target_agent_id = Some(target.to_owned());
            next.handoff_id = Some(handoff_id);
            next.handoff_generation += 1;
            next.content_digest = None;
            let digest = digest_record(&next)?;
            next.content_digest = Some(digest);
            if self.cas(task_id, Some(&expected), &next)? {
                return envelope(next);
            }
        }
    }

    pub fn accept_handoff(
        &self,
        envelope: &HandoffEnvelope,
        target: &str,
    ) -> Result<CustodyRecord, HandoffError> {
        envelope.verify()?;
        if envelope.record.custody_state != CustodyState::TransferPending
            || envelope.record.target_agent_id.as_deref() != Some(target)
        {
            return Err(HandoffError::InvalidTransition("target mismatch".into()));
        }
        let mut accepted = envelope.record.clone();
        accepted.owner_agent_id = target.to_owned();
        accepted.target_agent_id = None;
        accepted.custody_state = CustodyState::Owned;
        let id = accepted.task.task_id.clone();
        if self.cas(&id, None, &accepted)? {
            return Ok(accepted);
        }
        let existing = self
            .inspect(&id)?
            .ok_or_else(|| HandoffError::NotFound(id.clone()))?;
        if existing == accepted {
            Ok(existing)
        } else {
            Err(HandoffError::Conflict)
        }
    }

    pub fn claim_task(
        &self,
        task_id: &str,
        agent_id: &str,
        worker_id: &str,
        generation: u64,
    ) -> Result<Claim, HandoffError> {
        loop {
            let record = self
                .read_record(task_id)?
                .ok_or_else(|| HandoffError::NotFound(task_id.into()))?;
            if record.execution_state == ExecutionState::InProgress {
                if record.owner_agent_id == agent_id
                    && record.claimed_by_worker.as_deref() == Some(worker_id)
                {
                    if record.handoff_generation != generation {
                        return Err(HandoffError::InvalidTransition("generation mismatch".into()));
                    }
                    return Ok(Claim {
                        task_id: task_id.into(),
                        worker_id: worker_id.into(),
                        token: record.claim_token.ok_or(HandoffError::InvalidClaimToken)?,
                        generation: record.handoff_generation,
                    });
                }
                return Err(HandoffError::AlreadyClaimed);
            }
            if record.custody_state != CustodyState::Owned
                || record.execution_state != ExecutionState::Open
                || record.owner_agent_id != agent_id
                || record.handoff_generation != generation
            {
                return Err(HandoffError::InvalidTransition(
                    "task is not evaluable by this owner/generation".into(),
                ));
            }
            let expected = record.clone();
            let mut next = record;
            let token = Uuid::new_v4();
            next.execution_state = ExecutionState::InProgress;
            next.claimed_by_worker = Some(worker_id.to_owned());
            next.claim_token = Some(token);
            if self.cas(task_id, Some(&expected), &next)? {
                return Ok(Claim {
                    task_id: task_id.into(),
                    worker_id: worker_id.into(),
                    token,
                    generation,
                });
            }
        }
    }

    pub fn set_blocked(
        &self,
        task_id: &str,
        token: Uuid,
        reason: &str,
    ) -> Result<CustodyRecord, HandoffError> {
        validate_nonempty("blocked reason", reason)?;
        self.update_claimed(
            task_id,
            token,
            ExecutionState::Blocked,
            Some(reason.to_owned()),
            None,
        )
    }

    pub fn complete_claimed(
        &self,
        task_id: &str,
        token: Uuid,
        result: String,
    ) -> Result<CustodyRecord, HandoffError> {
        self.update_claimed(
            task_id,
            token,
            ExecutionState::Completed,
            None,
            Some(result),
        )
    }

    pub fn fail_claimed(
        &self,
        task_id: &str,
        token: Uuid,
        error: String,
    ) -> Result<CustodyRecord, HandoffError> {
        self.update_claimed(task_id, token, ExecutionState::Failed, None, Some(error))
    }

    fn update_claimed(
        &self,
        task_id: &str,
        token: Uuid,
        next_state: ExecutionState,
        blocked_reason: Option<String>,
        result: Option<String>,
    ) -> Result<CustodyRecord, HandoffError> {
        loop {
            let record = self
                .read_record(task_id)?
                .ok_or_else(|| HandoffError::NotFound(task_id.into()))?;
            if record.claim_token != Some(token) {
                return Err(HandoffError::InvalidClaimToken);
            }
            if record.execution_state.terminal() {
                if record.execution_state == next_state
                    && record.blocked_reason == blocked_reason
                    && record.result == result
                {
                    return Ok(record);
                }
                return Err(HandoffError::InvalidTransition(
                    "terminal state is monotonic".into(),
                ));
            }
            if record.execution_state != ExecutionState::InProgress {
                return Err(HandoffError::InvalidTransition(
                    "only an in-progress claim may be updated".into(),
                ));
            }
            let expected = record.clone();
            let mut next = record;
            next.execution_state = next_state.clone();
            next.blocked_reason = blocked_reason.clone();
            next.result = result.clone();
            if self.cas(task_id, Some(&expected), &next)? {
                return Ok(next);
            }
        }
    }

    pub fn evaluable_tasks(&self, agent_id: &str) -> Result<Vec<CustodyRecord>, HandoffError> {
        let mut out = Vec::new();
        for r in self.crdt.list() {
            if !r.id.starts_with(TASK_PREFIX) || r.data.is_null() {
                continue;
            }
            let record: CustodyRecord = match serde_json::from_value(r.data) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if record.owner_agent_id == agent_id
                && record.custody_state == CustodyState::Owned
                && record.execution_state == ExecutionState::Open
            {
                out.push(record);
            }
        }
        out.sort_by(|a, b| a.task.task_id.cmp(&b.task.task_id));
        Ok(out)
    }
}


fn validate_nonempty(field: &str, value: &str) -> Result<(), HandoffError> {
    if value.trim().is_empty() {
        Err(HandoffError::InvalidTransition(format!("{field} is empty")))
    } else {
        Ok(())
    }
}

fn digest_task(task: &TransferableTask) -> Result<String, HandoffError> {
    Ok(blake3::hash(&serde_json::to_vec(task)?)
        .to_hex()
        .to_string())
}

fn digest_record(record: &CustodyRecord) -> Result<String, HandoffError> {
    let mut unsigned = record.clone();
    unsigned.content_digest = None;
    Ok(blake3::hash(&serde_json::to_vec(&unsigned)?)
        .to_hex()
        .to_string())
}

fn envelope(record: CustodyRecord) -> Result<HandoffEnvelope, HandoffError> {
    let digest = digest_record(&record)?;
    if record.content_digest.as_deref() != Some(&digest) {
        return Err(HandoffError::IntegrityConflict);
    }
    Ok(HandoffEnvelope {
        schema: SCHEMA.into(),
        record,
        digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;

    fn path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("radix-handoff-{name}-{}", Uuid::new_v4()))
    }

    fn task() -> TransferableTask {
        TransferableTask {
            task_id: "TASK-ATOMIC".into(),
            objective: "ship one bounded change".into(),
            repo: "plures/example".into(),
            priority: "P1".into(),
            constraints: vec!["no stubs".into()],
            acceptance_criteria: vec!["tests pass".into()],
            next_action: "implement".into(),
            provenance: "certification".into(),
            artifacts: vec![],
        }
    }

    #[test]
    fn export_import_are_idempotent_and_owner_filtered() {
        let source_path = path("source");
        let target_path = path("target");
        let source = ConditionalTaskStore::open(&source_path).unwrap();
        let target = ConditionalTaskStore::open(&target_path).unwrap();
        source.seed_owned(task(), "openclaw").unwrap();
        let id = Uuid::new_v4();
        let first = source
            .prepare_handoff("TASK-ATOMIC", "openclaw", "praxisbot", id, 0)
            .unwrap();
        let second = source
            .prepare_handoff("TASK-ATOMIC", "openclaw", "praxisbot", id, 0)
            .unwrap();
        assert_eq!(
            first.canonical_json().unwrap(),
            second.canonical_json().unwrap()
        );
        assert!(source.evaluable_tasks("openclaw").unwrap().is_empty());
        let accepted = target.accept_handoff(&first, "praxisbot").unwrap();
        assert_eq!(
            accepted,
            target.accept_handoff(&first, "praxisbot").unwrap()
        );
        assert_eq!(target.evaluable_tasks("praxisbot").unwrap().len(), 1);
        drop(source);
        drop(target);
        let _ = std::fs::remove_dir_all(source_path);
        let _ = std::fs::remove_dir_all(target_path);
    }

    #[test]
    fn duplicate_claim_has_one_winner_and_token_guards_updates_across_restart() {
        let store_path = path("claim");
        let source = ConditionalTaskStore::open(path("claim-source")).unwrap();
        let store = ConditionalTaskStore::open(&store_path).unwrap();
        source.seed_owned(task(), "openclaw").unwrap();
        let env = source
            .prepare_handoff("TASK-ATOMIC", "openclaw", "praxisbot", Uuid::new_v4(), 0)
            .unwrap();
        store.accept_handoff(&env, "praxisbot").unwrap();
        let store = Arc::new(store);
        let barrier = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();
        for worker in ["a", "b"] {
            let s = store.clone();
            let b = barrier.clone();
            let w = worker.to_owned();
            threads.push(thread::spawn(move || {
                b.wait();
                s.claim_task("TASK-ATOMIC", "praxisbot", &w, 1)
            }));
        }
        barrier.wait();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        let winner = results.into_iter().find_map(Result::ok).unwrap();
        assert!(matches!(
            store.complete_claimed("TASK-ATOMIC", Uuid::new_v4(), "bad".into()),
            Err(HandoffError::InvalidClaimToken)
        ));
        drop(store);
        let reopened = ConditionalTaskStore::open(&store_path).unwrap();
        let done = reopened
            .complete_claimed("TASK-ATOMIC", winner.token, "ok".into())
            .unwrap();
        assert_eq!(done.execution_state, ExecutionState::Completed);
        drop(reopened);
        let _ = std::fs::remove_dir_all(store_path);
    }

    #[test]
    fn blocked_requires_reason_and_is_durable() {
        let store_path = path("blocked");
        let store = ConditionalTaskStore::open(&store_path).unwrap();
        store.seed_owned(task(), "praxisbot").unwrap();
        let claim = store
            .claim_task("TASK-ATOMIC", "praxisbot", "worker", 0)
            .unwrap();
        assert!(store.set_blocked("TASK-ATOMIC", claim.token, " ").is_err());
        store
            .set_blocked("TASK-ATOMIC", claim.token, "waiting for review")
            .unwrap();
        drop(store);
        let reopened = ConditionalTaskStore::open(&store_path).unwrap();
        let record = reopened.inspect("TASK-ATOMIC").unwrap().unwrap();
        assert_eq!(record.execution_state, ExecutionState::Blocked);
        assert_eq!(record.blocked_reason.as_deref(), Some("waiting for review"));
        assert!(reopened.evaluable_tasks("praxisbot").unwrap().is_empty());
        drop(reopened);
        let _ = std::fs::remove_dir_all(store_path);
    }
}
