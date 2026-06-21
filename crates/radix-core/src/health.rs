//! Startup health reporting — records subsystem status to Chronos and PluresDB.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use pluresdb::CrdtStore;
use serde::{Deserialize, Serialize};

use crate::chronos::{ChronosAction, ChronosTimeline};

/// Status of a subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Ok,
    Degraded(String),
    Failed(String),
}

impl HealthStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Ok => "✅",
            Self::Degraded(_) => "⚠️",
            Self::Failed(_) => "❌",
        }
    }
}

/// A single subsystem health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub subsystem: String,
    pub status: HealthStatus,
    pub message: String,
    pub timestamp: u64,
}

/// Aggregated system health state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemHealth {
    pub reports: Vec<HealthReport>,
}

impl SystemHealth {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a subsystem health check.
    pub fn record(&mut self, subsystem: &str, status: HealthStatus, message: &str) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.reports.push(HealthReport {
            subsystem: subsystem.to_string(),
            status,
            message: message.to_string(),
            timestamp,
        });
    }

    /// Check if ALL subsystems are healthy.
    pub fn all_ok(&self) -> bool {
        self.reports.iter().all(|r| r.status.is_ok())
    }

    /// Get degraded or failed subsystems.
    pub fn issues(&self) -> Vec<&HealthReport> {
        self.reports.iter().filter(|r| !r.status.is_ok()).collect()
    }

    /// Format as a human-readable status report.
    pub fn report(&self) -> String {
        let mut lines = Vec::with_capacity(self.reports.len() + 1);
        lines.push("System Health Report:".to_string());
        for r in &self.reports {
            lines.push(format!(
                "  {} {}: {}",
                r.status.emoji(),
                r.subsystem,
                r.message
            ));
        }
        lines.join("\n")
    }

    /// Format for Telegram (with emoji status indicators).
    pub fn telegram_report(&self) -> String {
        let mut lines = Vec::with_capacity(self.reports.len() + 2);
        lines.push("<b>System Health</b>".to_string());
        lines.push(String::new());
        for r in &self.reports {
            lines.push(format!(
                "{} <b>{}</b>: {}",
                r.status.emoji(),
                r.subsystem,
                r.message
            ));
        }
        if self.all_ok() {
            lines.push(String::new());
            lines.push("All systems operational.".to_string());
        }
        lines.join("\n")
    }

    /// Write all health reports to Chronos.
    pub fn persist_to_chronos(&self, chronos: &ChronosTimeline) {
        for report in &self.reports {
            let data = serde_json::to_value(report).unwrap_or_default();
            let entry = chronos.build_entry(
                &format!("health:{}", report.subsystem),
                "system",
                ChronosAction::Update,
                &data,
                vec![],
                Some(format!(
                    "Health check: {} = {:?}",
                    report.subsystem, report.status
                )),
            );
            chronos.record(&entry);
        }
    }

    /// Write to PluresDB as queryable state.
    pub fn persist_to_store(&self, store: &Arc<CrdtStore>) {
        let data = serde_json::to_value(self).unwrap_or_default();
        store.put("health:system".to_string(), "system", data);

        // Also write individual subsystem entries for targeted queries.
        for report in &self.reports {
            let data = serde_json::to_value(report).unwrap_or_default();
            store.put(
                format!("health:subsystem:{}", report.subsystem),
                "system",
                data,
            );
        }
    }

    /// Load the last persisted health from PluresDB.
    pub fn load_from_store(store: &Arc<CrdtStore>) -> Option<Self> {
        store
            .get("health:system")
            .and_then(|record| serde_json::from_value(record.data).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_reporting() {
        let mut health = SystemHealth::new();
        health.record("pluresdb", HealthStatus::Ok, "Store opened");
        health.record(
            "embeddings",
            HealthStatus::Degraded("MockEmbedder".into()),
            "No semantic search",
        );
        health.record(
            "copilot",
            HealthStatus::Failed("timeout".into()),
            "Cannot authenticate",
        );

        assert!(!health.all_ok());
        assert_eq!(health.issues().len(), 2);

        let report = health.report();
        assert!(report.contains("✅ pluresdb"));
        assert!(report.contains("⚠️ embeddings"));
        assert!(report.contains("❌ copilot"));
    }

    #[test]
    fn test_all_ok() {
        let mut health = SystemHealth::new();
        health.record("a", HealthStatus::Ok, "fine");
        health.record("b", HealthStatus::Ok, "fine");
        assert!(health.all_ok());
    }

    #[test]
    fn test_serialization() {
        let mut health = SystemHealth::new();
        health.record("test", HealthStatus::Ok, "works");
        let json = serde_json::to_string(&health).unwrap();
        let restored: SystemHealth = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.reports.len(), 1);
    }
}
