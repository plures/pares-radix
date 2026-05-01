//! Praxis write gate — validates DATA before persistence to CrdtStore.
//!
//! This is distinct from the action-level [`PraxisGate`] (which checks
//! whether a *procedure* should execute).  The write gate checks the
//! *data* being persisted and can reject or warn on writes that violate
//! constraints (secrets in payloads, oversized nodes, etc.).

use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Metadata describing a write constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteConstraint {
    pub id: String,
    pub name: String,
    pub description: String,
    pub severity: WriteSeverity,
    pub enabled: bool,
}

/// Whether a failed constraint blocks the write or merely warns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WriteSeverity {
    Error,
    Warning,
}

/// Returned when a write is blocked by an `Error`-severity constraint.
#[derive(Debug, Clone)]
pub struct WriteRejection {
    pub constraint_id: String,
    pub constraint_name: String,
    pub reason: String,
}

impl std::fmt::Display for WriteRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Write blocked by '{}' ({}): {}",
            self.constraint_name, self.constraint_id, self.reason
        )
    }
}

impl std::error::Error for WriteRejection {}

/// A single decision-log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEntry {
    pub timestamp: u64,
    pub key: String,
    pub constraint_id: String,
    pub passed: bool,
    pub reason: Option<String>,
    pub actor: String,
}

/// Type-erased constraint checker.
pub trait WriteCheck: Send + Sync {
    fn check(&self, key: &str, data: &Value) -> Result<(), String>;
}

// ---------------------------------------------------------------------------
// Gate
// ---------------------------------------------------------------------------

/// The write gate holds a list of constraints and a decision log.
pub struct PraxisWriteGate {
    constraints: RwLock<Vec<(WriteConstraint, Box<dyn WriteCheck>)>>,
    decision_log: RwLock<Vec<DecisionEntry>>,
    max_log_size: usize,
}

impl PraxisWriteGate {
    /// Create a new gate pre-seeded with default constraints.
    pub fn new() -> Self {
        let mut gate = Self {
            constraints: RwLock::new(Vec::new()),
            decision_log: RwLock::new(Vec::new()),
            max_log_size: 1000,
        };
        gate.seed_defaults();
        gate
    }

    fn seed_defaults(&mut self) {
        self.add_constraint(
            WriteConstraint {
                id: "praxis:no-secrets".into(),
                name: "No secrets in data".into(),
                description: "Blocks writes containing API keys, tokens, or private keys".into(),
                severity: WriteSeverity::Error,
                enabled: true,
            },
            Box::new(NoSecretsCheck),
        );
        self.add_constraint(
            WriteConstraint {
                id: "praxis:max-size".into(),
                name: "Maximum node size (1MB)".into(),
                description: "Blocks writes larger than 1MB".into(),
                severity: WriteSeverity::Error,
                enabled: true,
            },
            Box::new(MaxSizeCheck(1_048_576)),
        );
    }

    /// Register a new constraint.
    pub fn add_constraint(&mut self, meta: WriteConstraint, check: Box<dyn WriteCheck>) {
        self.constraints.write().unwrap().push((meta, check));
    }

    /// Evaluate all enabled constraints against a proposed write.
    ///
    /// Returns `Ok(warnings)` on success or `Err(rejection)` if an
    /// `Error`-severity constraint fails.
    pub fn evaluate(
        &self,
        key: &str,
        data: &Value,
        actor: &str,
    ) -> Result<Vec<String>, WriteRejection> {
        let constraints = self.constraints.read().unwrap();
        let mut warnings = Vec::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for (meta, check) in constraints.iter() {
            if !meta.enabled {
                continue;
            }
            match check.check(key, data) {
                Ok(()) => {
                    self.log_decision(DecisionEntry {
                        timestamp: now,
                        key: key.to_string(),
                        constraint_id: meta.id.clone(),
                        passed: true,
                        reason: None,
                        actor: actor.to_string(),
                    });
                }
                Err(reason) => {
                    self.log_decision(DecisionEntry {
                        timestamp: now,
                        key: key.to_string(),
                        constraint_id: meta.id.clone(),
                        passed: false,
                        reason: Some(reason.clone()),
                        actor: actor.to_string(),
                    });
                    match meta.severity {
                        WriteSeverity::Error => {
                            return Err(WriteRejection {
                                constraint_id: meta.id.clone(),
                                constraint_name: meta.name.clone(),
                                reason,
                            })
                        }
                        WriteSeverity::Warning => {
                            warnings.push(format!("[{}] {}", meta.name, reason));
                        }
                    }
                }
            }
        }
        Ok(warnings)
    }

    fn log_decision(&self, entry: DecisionEntry) {
        let mut log = self.decision_log.write().unwrap();
        if log.len() >= self.max_log_size {
            log.remove(0);
        }
        log.push(entry);
    }

    /// Return the most recent `limit` decision entries (newest first).
    pub fn recent_decisions(&self, limit: usize) -> Vec<DecisionEntry> {
        let log = self.decision_log.read().unwrap();
        log.iter().rev().take(limit).cloned().collect()
    }

    /// Return the most recent `limit` *failed* decision entries.
    pub fn violations(&self, limit: usize) -> Vec<DecisionEntry> {
        let log = self.decision_log.read().unwrap();
        log.iter()
            .rev()
            .filter(|e| !e.passed)
            .take(limit)
            .cloned()
            .collect()
    }

    /// List all registered constraint metadata.
    pub fn list_constraints(&self) -> Vec<WriteConstraint> {
        self.constraints
            .read()
            .unwrap()
            .iter()
            .map(|(m, _)| m.clone())
            .collect()
    }
}

impl Default for PraxisWriteGate {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in checks
// ---------------------------------------------------------------------------

struct NoSecretsCheck;

impl WriteCheck for NoSecretsCheck {
    fn check(&self, _key: &str, data: &Value) -> Result<(), String> {
        let text = serde_json::to_string(data).unwrap_or_default();
        let patterns = [
            "ghp_",
            "gho_",
            "sk-",
            "-----BEGIN PRIVATE KEY",
            "-----BEGIN RSA PRIVATE",
            "AKIA",
            "password=",
            "secret=",
            "token=",
        ];
        for p in &patterns {
            if text.contains(p) {
                return Err(format!(
                    "Data contains potential secret matching pattern '{p}'"
                ));
            }
        }
        Ok(())
    }
}

struct MaxSizeCheck(usize);

impl WriteCheck for MaxSizeCheck {
    fn check(&self, _key: &str, data: &Value) -> Result<(), String> {
        let size = serde_json::to_string(data).unwrap_or_default().len();
        if size > self.0 {
            return Err(format!("Data size {size} exceeds limit {}", self.0));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clean_write_passes() {
        let gate = PraxisWriteGate::new();
        let data = json!({"name": "test", "value": 42});
        let result = gate.evaluate("test:key", &data, "actor-1");
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn secrets_check_blocks_ghp_token() {
        let gate = PraxisWriteGate::new();
        let data = json!({"token": "ghp_abc123secretvalue"});
        let result = gate.evaluate("test:key", &data, "actor-1");
        assert!(result.is_err());
        let rejection = result.unwrap_err();
        assert_eq!(rejection.constraint_id, "praxis:no-secrets");
        assert!(rejection.reason.contains("ghp_"));
    }

    #[test]
    fn secrets_check_blocks_private_key() {
        let gate = PraxisWriteGate::new();
        let data = json!({"key": "-----BEGIN PRIVATE KEY-----\nfoo\n-----END PRIVATE KEY-----"});
        let result = gate.evaluate("test:key", &data, "actor-1");
        assert!(result.is_err());
    }

    #[test]
    fn max_size_check_blocks_oversized() {
        let gate = PraxisWriteGate::new();
        // Create data > 1MB
        let big = "x".repeat(1_100_000);
        let data = json!({"payload": big});
        let result = gate.evaluate("test:key", &data, "actor-1");
        assert!(result.is_err());
        let rejection = result.unwrap_err();
        assert_eq!(rejection.constraint_id, "praxis:max-size");
    }

    #[test]
    fn warning_constraint_allows_write_but_logs() {
        let mut gate = PraxisWriteGate::new();
        gate.add_constraint(
            WriteConstraint {
                id: "test:warn-only".into(),
                name: "Warn on test key".into(),
                description: "Warns when key starts with 'warn:'".into(),
                severity: WriteSeverity::Warning,
                enabled: true,
            },
            Box::new(WarnOnPrefixCheck("warn:")),
        );
        let data = json!({"x": 1});
        let result = gate.evaluate("warn:something", &data, "actor-1");
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Warn on test key"));
    }

    #[test]
    fn decision_log_captures_entries() {
        let gate = PraxisWriteGate::new();
        let data = json!({"clean": true});
        let _ = gate.evaluate("k1", &data, "a");
        let _ = gate.evaluate("k2", &data, "a");
        let decisions = gate.recent_decisions(10);
        // 2 keys × 2 constraints each = 4 entries
        assert_eq!(decisions.len(), 4);
        assert!(decisions.iter().all(|d| d.passed));
    }

    #[test]
    fn violations_filter_works() {
        let gate = PraxisWriteGate::new();
        let clean = json!({"ok": true});
        let dirty = json!({"t": "ghp_leaked"});
        let _ = gate.evaluate("k1", &clean, "a");
        let _ = gate.evaluate("k2", &dirty, "a");
        let violations = gate.violations(10);
        assert_eq!(violations.len(), 1);
        assert!(!violations[0].passed);
        assert_eq!(violations[0].key, "k2");
    }

    #[test]
    fn disabled_constraint_is_skipped() {
        let gate = PraxisWriteGate::new();
        // Disable the no-secrets check
        {
            let mut constraints = gate.constraints.write().unwrap();
            constraints[0].0.enabled = false;
        }
        let data = json!({"t": "ghp_should_pass_now"});
        let result = gate.evaluate("k", &data, "a");
        assert!(result.is_ok());
    }

    // Helper for warning test
    struct WarnOnPrefixCheck(&'static str);
    impl WriteCheck for WarnOnPrefixCheck {
        fn check(&self, key: &str, _data: &Value) -> Result<(), String> {
            if key.starts_with(self.0) {
                Err(format!("Key starts with '{}'", self.0))
            } else {
                Ok(())
            }
        }
    }
}
