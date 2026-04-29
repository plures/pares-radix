use serde::{Deserialize, Serialize};

use crate::px_parser::PxWorkload;

/// Runtime state for a deployed workload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadState {
    pub name: String,
    pub spec: PxWorkload,
    pub assigned_nodes: Vec<String>,
    pub health_failures: u32,
    pub cpu_avg: f64,
    pub status: WorkloadStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkloadStatus {
    Pending,
    Running,
    Degraded,
    Failed,
}

impl WorkloadState {
    pub fn new(spec: PxWorkload) -> Self {
        Self {
            name: spec.name.clone(),
            spec,
            assigned_nodes: vec![],
            health_failures: 0,
            cpu_avg: 0.0,
            status: WorkloadStatus::Pending,
        }
    }

    pub fn record_health_failure(&mut self) {
        self.health_failures += 1;
    }

    pub fn reset_health(&mut self) {
        self.health_failures = 0;
    }
}
