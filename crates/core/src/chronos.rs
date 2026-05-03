//! Chronos version timeline — records every data mutation with causal chains.
//!
//! Every write through the [`PraxisWriteGate`] produces a [`ChronosEntry`]
//! that captures who, what, when, why, and the constraint evaluation results.
//! Entries are stored in PluresDB under the `chronos:` key prefix and linked
//! into per-key causal chains via `parent_id`.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use pluresdb::CrdtStore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

/// The PluresDB actor used for Chronos writes.
const CHRONOS_ACTOR: &str = "chronos";

/// A version timeline entry — records every data mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChronosEntry {
    pub id: String,
    pub timestamp: u64,
    pub actor: String,
    pub key: String,
    pub action: ChronosAction,
    pub data_hash: String,
    pub parent_id: Option<String>,
    pub rationale: Option<String>,
    pub constraint_results: Vec<String>,
}

/// The kind of mutation recorded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChronosAction {
    Create,
    Update,
    Delete,
    Move,
    /// Agent received a user message.
    MessageReceived,
    /// Agent generated a response.
    ResponseGenerated,
    /// A tool was invoked.
    ToolInvoked,
    /// Context manager adjusted the window.
    ContextManaged,
    /// Model was called (conscious/deep/bitnet).
    ModelCalled,
    /// Outcome recorded (user correction or acceptance).
    OutcomeRecorded,
}

impl std::fmt::Display for ChronosAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create => write!(f, "Create"),
            Self::Update => write!(f, "Update"),
            Self::Delete => write!(f, "Delete"),
            Self::Move => write!(f, "Move"),
            Self::MessageReceived => write!(f, "MessageReceived"),
            Self::ResponseGenerated => write!(f, "ResponseGenerated"),
            Self::ToolInvoked => write!(f, "ToolInvoked"),
            Self::ContextManaged => write!(f, "ContextManaged"),
            Self::ModelCalled => write!(f, "ModelCalled"),
            Self::OutcomeRecorded => write!(f, "OutcomeRecorded"),
        }
    }
}

/// Causal version timeline backed by PluresDB.
///
/// Stores to PluresDB (primary) and optionally writes JSONL to a file sink
/// for cross-machine debugging and analysis.
pub struct ChronosTimeline {
    store: Arc<CrdtStore>,
    /// Optional JSONL output directory. When set, every record() also
    /// appends the entry as one JSON line to `<dir>/YYYY-MM-DD.jsonl`.
    jsonl_dir: Option<std::path::PathBuf>,
}

impl ChronosTimeline {
    /// Create a new timeline backed by the given store.
    pub fn new(store: Arc<CrdtStore>) -> Self {
        Self { store, jsonl_dir: None }
    }

    /// Create a timeline with JSONL file output.
    pub fn with_jsonl(store: Arc<CrdtStore>, dir: std::path::PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        Self { store, jsonl_dir: Some(dir) }
    }

    /// Enable JSONL output from an environment variable.
    pub fn with_jsonl_from_env(store: Arc<CrdtStore>) -> Self {
        if let Ok(dir) = std::env::var("PARES_TELEMETRY_DIR") {
            let path = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&path).ok();
            tracing::info!(dir = %path.display(), "chronos JSONL output enabled");
            Self { store, jsonl_dir: Some(path) }
        } else {
            Self { store, jsonl_dir: None }
        }
    }

    /// Build a new [`ChronosEntry`] for a write, automatically resolving the
    /// causal parent from the latest entry for this key.
    pub fn build_entry(
        &self,
        key: &str,
        actor: &str,
        action: ChronosAction,
        data: &Value,
        constraint_results: Vec<String>,
        rationale: Option<String>,
    ) -> ChronosEntry {
        let parent_id = self.latest(key).map(|e| e.id);
        let data_hash = sha256_json(data);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        ChronosEntry {
            id: Uuid::new_v4().to_string(),
            timestamp,
            actor: actor.to_string(),
            key: key.to_string(),
            action,
            data_hash,
            parent_id,
            rationale,
            constraint_results,
        }
    }

    /// Record a mutation in the timeline.
    pub fn record(&self, entry: &ChronosEntry) {
        let entry_key = format!("chronos:entry:{}", entry.id);
        self.store.put(
            entry_key,
            CHRONOS_ACTOR,
            serde_json::to_value(entry).expect("ChronosEntry serializes"),
        );

        // Update the "latest" pointer for this data key.
        let latest_key = format!("chronos:latest:{}", entry.key);
        self.store.put(
            latest_key,
            CHRONOS_ACTOR,
            json!({ "entry_id": entry.id, "timestamp": entry.timestamp }),
        );

        // JSONL file sink — one line per entry, daily files.
        if let Some(ref dir) = self.jsonl_dir {
            if let Ok(json) = serde_json::to_string(entry) {
                let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
                let path = dir.join(format!("{date}.jsonl"));
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                {
                    use std::io::Write;
                    let _ = writeln!(file, "{json}");
                }
            }
        }
    }

    /// Get the version history for a key (newest first), up to `limit`.
    pub fn history(&self, data_key: &str, limit: usize) -> Vec<ChronosEntry> {
        let mut entries: Vec<ChronosEntry> = self
            .store
            .list()
            .into_iter()
            .filter_map(|r| {
                let e: ChronosEntry = serde_json::from_value(r.data).ok()?;
                if e.key == data_key {
                    Some(e)
                } else {
                    None
                }
            })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
        entries.truncate(limit);
        entries
    }

    /// Get the latest entry for a key.
    pub fn latest(&self, data_key: &str) -> Option<ChronosEntry> {
        let latest_key = format!("chronos:latest:{data_key}");
        let pointer = self.store.get(&latest_key)?;
        let entry_id = pointer.data.get("entry_id")?.as_str()?;
        let entry_key = format!("chronos:entry:{entry_id}");
        let record = self.store.get(&entry_key)?;
        serde_json::from_value(record.data).ok()
    }

    /// Get all entries by an actor (newest first), up to `limit`.
    pub fn by_actor(&self, actor: &str, limit: usize) -> Vec<ChronosEntry> {
        let mut entries: Vec<ChronosEntry> = self
            .store
            .list()
            .into_iter()
            .filter_map(|r| {
                let e: ChronosEntry = serde_json::from_value(r.data).ok()?;
                if e.actor == actor {
                    Some(e)
                } else {
                    None
                }
            })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
        entries.truncate(limit);
        entries
    }

    /// Get entries in a time range (newest first), up to `limit`.
    pub fn in_range(&self, from: u64, to: u64, limit: usize) -> Vec<ChronosEntry> {
        let mut entries: Vec<ChronosEntry> = self
            .store
            .list()
            .into_iter()
            .filter_map(|r| {
                let e: ChronosEntry = serde_json::from_value(r.data).ok()?;
                if e.timestamp >= from && e.timestamp <= to {
                    Some(e)
                } else {
                    None
                }
            })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
        entries.truncate(limit);
        entries
    }

    /// Recent entries across all keys (newest first), up to `limit`.
    pub fn recent(&self, limit: usize) -> Vec<ChronosEntry> {
        let mut entries: Vec<ChronosEntry> = self
            .store
            .list()
            .into_iter()
            .filter_map(|r| serde_json::from_value(r.data).ok())
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
        entries.truncate(limit);
        entries
    }
}

/// SHA-256 hash of a JSON value (deterministic via to_string).
fn sha256_json(data: &Value) -> String {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(data).unwrap_or_default();
    format!("{:x}", Sha256::digest(&bytes))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Arc<CrdtStore> {
        Arc::new(CrdtStore::default())
    }

    #[test]
    fn record_and_latest() {
        let store = test_store();
        let timeline = ChronosTimeline::new(store);

        let entry = timeline.build_entry(
            "test:key1",
            "actor-1",
            ChronosAction::Create,
            &json!({"hello": "world"}),
            vec!["praxis:no-secrets: pass".into()],
            Some("initial creation".into()),
        );
        assert!(entry.parent_id.is_none());
        timeline.record(&entry);

        let latest = timeline.latest("test:key1");
        assert!(latest.is_some());
        let latest = latest.unwrap();
        assert_eq!(latest.id, entry.id);
        assert_eq!(latest.action, ChronosAction::Create);
    }

    #[test]
    fn causal_chain() {
        let store = test_store();
        let timeline = ChronosTimeline::new(store);

        let e1 = timeline.build_entry(
            "k",
            "a",
            ChronosAction::Create,
            &json!(1),
            vec![],
            None,
        );
        timeline.record(&e1);

        let e2 = timeline.build_entry(
            "k",
            "a",
            ChronosAction::Update,
            &json!(2),
            vec![],
            None,
        );
        assert_eq!(e2.parent_id.as_deref(), Some(e1.id.as_str()));
        timeline.record(&e2);

        let hist = timeline.history("k", 10);
        assert_eq!(hist.len(), 2);
        // Both entries should be present; latest pointer should be e2.
        let latest = timeline.latest("k").unwrap();
        assert_eq!(latest.id, e2.id);
    }

    #[test]
    fn by_actor_filter() {
        let store = test_store();
        let timeline = ChronosTimeline::new(store);

        let e1 = timeline.build_entry("k1", "alice", ChronosAction::Create, &json!(1), vec![], None);
        timeline.record(&e1);
        let e2 = timeline.build_entry("k2", "bob", ChronosAction::Create, &json!(2), vec![], None);
        timeline.record(&e2);

        assert_eq!(timeline.by_actor("alice", 10).len(), 1);
        assert_eq!(timeline.by_actor("bob", 10).len(), 1);
        assert_eq!(timeline.by_actor("nobody", 10).len(), 0);
    }

    #[test]
    fn recent_returns_all() {
        let store = test_store();
        let timeline = ChronosTimeline::new(store);

        for i in 0..5 {
            let e = timeline.build_entry(
                &format!("k{i}"),
                "a",
                ChronosAction::Create,
                &json!(i),
                vec![],
                None,
            );
            timeline.record(&e);
        }

        assert_eq!(timeline.recent(3).len(), 3);
        assert_eq!(timeline.recent(10).len(), 5);
    }
}
