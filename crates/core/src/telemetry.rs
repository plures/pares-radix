//! Chronos telemetry — records every agent interaction with causal links.
//!
//! Each interaction produces a `TelemetryRecord` that captures:
//! - What the user said
//! - What entities were extracted
//! - What context was in the window (with relevance scores)
//! - Which model was called and how long it took
//! - What tools were used
//! - What the agent responded
//! - Outcome signal (from subsequent user message)
//!
//! Records are stored as JSONL files at a configurable path,
//! and can be pushed to a git repo for cross-machine analysis.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// A single telemetry record for one agent interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryRecord {
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Hostname of the machine.
    pub node: String,
    /// Unique interaction ID.
    pub interaction_id: String,
    /// What the user said.
    pub user_message: String,
    /// Entities extracted by the context manager.
    pub entities: Vec<String>,
    /// Context items in the window with relevance scores.
    pub context_items: Vec<ContextSnapshot>,
    /// Which model was called.
    pub model: String,
    /// Model response latency in milliseconds.
    pub latency_ms: u64,
    /// Tools that were called during this interaction.
    pub tools_used: Vec<ToolUse>,
    /// What the agent responded.
    pub response: String,
    /// Response length in characters.
    pub response_len: usize,
    /// Route decision (conscious/deep/drop).
    pub route: String,
    /// Whether context was cleared (topic shift).
    pub topic_shifted: bool,
    /// Causal link to previous interaction.
    pub previous_interaction_id: Option<String>,
}

/// Snapshot of a context item at time of use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub id: String,
    pub relevance: f32,
    pub source: String,
    pub tokens: usize,
}

/// Record of a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUse {
    pub name: String,
    pub success: bool,
    pub latency_ms: u64,
}

/// Telemetry logger — appends records to a JSONL file.
pub struct TelemetryLogger {
    /// Directory for telemetry files.
    output_dir: PathBuf,
    /// Last interaction ID for causal chaining.
    last_interaction_id: Mutex<Option<String>>,
    /// Node hostname.
    node: String,
}

impl TelemetryLogger {
    /// Create a new logger writing to the given directory.
    pub fn new(output_dir: PathBuf) -> Self {
        let node = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".into());

        std::fs::create_dir_all(&output_dir).ok();

        Self {
            output_dir,
            last_interaction_id: Mutex::new(None),
            node,
        }
    }

    /// Log a complete interaction.
    pub fn log(&self, mut record: TelemetryRecord) {
        // Set node and causal link
        record.node = self.node.clone();
        record.previous_interaction_id = self
            .last_interaction_id
            .lock()
            .unwrap()
            .clone();

        // Update causal chain
        *self.last_interaction_id.lock().unwrap() =
            Some(record.interaction_id.clone());

        // Write to daily JSONL file
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let path = self.output_dir.join(format!("{date}.jsonl"));

        if let Ok(json) = serde_json::to_string(&record) {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(file, "{json}");
            }
        }

        tracing::debug!(
            interaction_id = %record.interaction_id,
            model = %record.model,
            latency_ms = record.latency_ms,
            tools = record.tools_used.len(),
            context_items = record.context_items.len(),
            response_len = record.response_len,
            "telemetry recorded"
        );
    }

    /// Get the output directory path.
    pub fn output_dir(&self) -> &PathBuf {
        &self.output_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let logger = TelemetryLogger::new(dir.path().to_path_buf());

        let record = TelemetryRecord {
            timestamp: Utc::now().to_rfc3339(),
            node: "test".into(),
            interaction_id: "test-001".into(),
            user_message: "What is 2+2?".into(),
            entities: vec![],
            context_items: vec![],
            model: "gpt-4.1".into(),
            latency_ms: 850,
            tools_used: vec![],
            response: "4".into(),
            response_len: 1,
            route: "conscious".into(),
            topic_shifted: false,
            previous_interaction_id: None,
        };

        logger.log(record.clone());

        // Verify file exists and contains the record
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let path = dir.path().join(format!("{date}.jsonl"));
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("test-001"));
        assert!(content.contains("What is 2+2?"));
    }

    #[test]
    fn test_causal_chain() {
        let dir = tempfile::tempdir().unwrap();
        let logger = TelemetryLogger::new(dir.path().to_path_buf());

        let r1 = TelemetryRecord {
            timestamp: Utc::now().to_rfc3339(),
            node: "test".into(),
            interaction_id: "chain-001".into(),
            user_message: "first".into(),
            entities: vec![],
            context_items: vec![],
            model: "test".into(),
            latency_ms: 100,
            tools_used: vec![],
            response: "ok".into(),
            response_len: 2,
            route: "conscious".into(),
            topic_shifted: false,
            previous_interaction_id: None,
        };
        logger.log(r1);

        let mut r2 = TelemetryRecord {
            timestamp: Utc::now().to_rfc3339(),
            node: "test".into(),
            interaction_id: "chain-002".into(),
            user_message: "second".into(),
            entities: vec![],
            context_items: vec![],
            model: "test".into(),
            latency_ms: 200,
            tools_used: vec![],
            response: "ok".into(),
            response_len: 2,
            route: "conscious".into(),
            topic_shifted: false,
            previous_interaction_id: None,
        };
        logger.log(r2.clone());

        // r2 should have previous_interaction_id = chain-001
        // (set by the logger internally)
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let path = dir.path().join(format!("{date}.jsonl"));
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let second: TelemetryRecord = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second.previous_interaction_id, Some("chain-001".into()));
    }
}
