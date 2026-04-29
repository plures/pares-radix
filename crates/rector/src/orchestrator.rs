use crate::deployer::{DeployError, DeployResult, NixDeployer};
use crate::node::ClusterNode;
use crate::px_parser::PxFile;
use crate::scheduler::{NodeAssignment, WorkloadScheduler};
use crate::workload::{WorkloadState, WorkloadStatus};

// ── Result types ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConstraintResult {
    pub constraint_name: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug)]
pub struct ReconcileResult {
    pub assignments: Vec<NodeAssignment>,
    pub blocked: Vec<ConstraintResult>,
}

#[derive(Debug, Clone)]
pub struct HealthResult {
    pub workload_name: String,
    pub healthy: bool,
}

#[derive(Debug, Clone)]
pub struct RuleAction {
    pub rule_name: String,
    pub triggered: bool,
    pub action: String,
    pub log_message: Option<String>,
}

// ── Orchestrator ──────────────────────────────────────────────────────

pub struct Orchestrator {
    pub config: PxFile,
    nodes: Vec<ClusterNode>,
    #[allow(dead_code)]
    workload_states: Vec<WorkloadState>,
}

impl Orchestrator {
    /// Build an orchestrator from a parsed PxFile and discovered nodes.
    pub fn new(config: PxFile, nodes: Vec<ClusterNode>) -> Self {
        let workload_states = config
            .workload
            .iter()
            .cloned()
            .map(WorkloadState::new)
            .collect();
        Self {
            config,
            nodes,
            workload_states,
        }
    }

    /// Parse a .px file and build the orchestrator.
    pub fn from_px_str(input: &str, nodes: Vec<ClusterNode>) -> Result<Self, crate::px_parser::ParseError> {
        let config = crate::px_parser::parse(input)?;
        Ok(Self::new(config, nodes))
    }

    // ── Constraint evaluation ─────────────────────────────────────

    /// Evaluate all constraints against cluster state.
    /// Currently does simple keyword matching on the check expression;
    /// a real implementation would use an expression evaluator.
    pub fn evaluate_constraints(&self) -> Vec<ConstraintResult> {
        let online_count = self.nodes.iter().filter(|n| n.is_schedulable()).count();
        let avg_cpu: f64 = if self.nodes.is_empty() {
            100.0
        } else {
            self.nodes.iter().map(|n| n.cpu_usage).sum::<f64>() / self.nodes.len() as f64
        };

        let mut results = Vec::new();

        // Node requirements
        for req in &self.config.node_requirement {
            let passed = evaluate_count_check(online_count, &req.count_check);
            results.push(ConstraintResult {
                constraint_name: req.name.clone(),
                passed,
                message: if passed {
                    format!("OK: {} nodes online", online_count)
                } else {
                    req.error.clone()
                },
            });
        }

        // Constraints
        for c in &self.config.constraint {
            // Simple heuristic: check for "healthy_nodes >= N" and "cpu_avg < N"
            let passed = evaluate_simple_check(online_count, avg_cpu, &c.check);
            results.push(ConstraintResult {
                constraint_name: c.name.clone(),
                passed,
                message: if passed {
                    "OK".into()
                } else {
                    c.message.clone()
                },
            });
        }

        results
    }

    /// Check if all gates for a workload pass.
    pub fn gates_pass(&self, gates: &[String]) -> bool {
        if gates.is_empty() {
            return true;
        }
        let results = self.evaluate_constraints();
        for gate in gates {
            if let Some(r) = results.iter().find(|r| r.constraint_name == *gate) {
                if !r.passed {
                    return false;
                }
            }
            // Unknown gate names are treated as passing (fail-open for extensibility)
        }
        true
    }

    // ── Reconcile ─────────────────────────────────────────────────

    /// Schedule workloads, respecting constraint gates.
    pub fn reconcile(&self) -> ReconcileResult {
        let scheduler = WorkloadScheduler::new(self.nodes.clone());
        let mut assignments = Vec::new();
        let mut blocked = Vec::new();

        for workload in &self.config.workload {
            if !self.gates_pass(&workload.gates) {
                blocked.push(ConstraintResult {
                    constraint_name: workload.gates.join(", "),
                    passed: false,
                    message: format!("Workload '{}' blocked by gate constraints", workload.name),
                });
                continue;
            }

            match scheduler.schedule(workload) {
                Ok(mut a) => assignments.append(&mut a),
                Err(e) => {
                    blocked.push(ConstraintResult {
                        constraint_name: "scheduling".into(),
                        passed: false,
                        message: format!("Workload '{}': {}", workload.name, e),
                    });
                }
            }
        }

        ReconcileResult {
            assignments,
            blocked,
        }
    }

    /// Deploy a single workload to a specific node.
    ///
    /// 1. Evaluate gate constraints
    /// 2. Deploy via [`NixDeployer`]
    /// 3. Return the result (PluresDB recording + Chronos logging are the
    ///    caller's responsibility until those subsystems are wired in).
    pub async fn deploy_workload(
        &self,
        workload: &crate::px_parser::PxWorkload,
        node: &ClusterNode,
    ) -> Result<DeployResult, DeployError> {
        // 1. Evaluate gates
        if !self.gates_pass(&workload.gates) {
            return Err(DeployError::NixRebuildFailed(format!(
                "gate constraints not met for workload '{}'",
                workload.name
            )));
        }

        // 2. Deploy via NixDeployer
        NixDeployer::deploy(node, workload).await

        // TODO: 3. Record in PluresDB
        // TODO: 4. Log to Chronos
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Parse simple ">= N" expressions against a count.
fn evaluate_count_check(actual: usize, check: &str) -> bool {
    let check = check.trim();
    if let Some(rest) = check.strip_prefix(">=") {
        rest.trim().parse::<usize>().map_or(false, |n| actual >= n)
    } else if let Some(rest) = check.strip_prefix("<=") {
        rest.trim().parse::<usize>().map_or(false, |n| actual <= n)
    } else if let Some(rest) = check.strip_prefix('>') {
        rest.trim().parse::<usize>().map_or(false, |n| actual > n)
    } else if let Some(rest) = check.strip_prefix('<') {
        rest.trim().parse::<usize>().map_or(false, |n| actual < n)
    } else if let Some(rest) = check.strip_prefix("==") {
        rest.trim().parse::<usize>().map_or(false, |n| actual == n)
    } else {
        false
    }
}

/// Very simple constraint check evaluator.
fn evaluate_simple_check(healthy_nodes: usize, cpu_avg: f64, check: &str) -> bool {
    // Parse "cluster.healthy_nodes >= 3 AND cluster.cpu_avg < 80"
    let parts: Vec<&str> = check.split(" AND ").collect();
    for part in parts {
        let part = part.trim();
        if part.contains("healthy_nodes") {
            // Extract the comparison
            if let Some(idx) = part.find(">=") {
                let n: usize = part[idx + 2..].trim().parse().unwrap_or(0);
                if healthy_nodes < n {
                    return false;
                }
            }
        } else if part.contains("cpu_avg") {
            if let Some(idx) = part.find('<') {
                let n: f64 = part[idx + 1..].trim().parse().unwrap_or(0.0);
                if cpu_avg >= n {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{ClusterNode, NodeCapabilities, NodeStatus};

    fn make_node(id: &str, cpu: f64) -> ClusterNode {
        ClusterNode {
            id: id.into(),
            hostname: format!("{id}.local"),
            addresses: vec![],
            capabilities: NodeCapabilities {
                cpu_cores: 8,
                memory_total_mb: 16384,
                gpu: false,
                gpu_model: None,
                os: "NixOS".into(),
                arch: "x86_64".into(),
                is_idle: false,
                bitnet_available: false,
            },
            status: NodeStatus::Online,
            workloads: vec![],
            last_seen: 0,
            cpu_usage: cpu,
        }
    }

    const PX: &str = r#"
[[node_requirement]]
name = "minimum_nodes"
count_check = ">= 3"
error = "Need 3+ nodes"

[[constraint]]
name = "deploy_safe"
check = "cluster.healthy_nodes >= 3 AND cluster.cpu_avg < 80"
severity = "error"
message = "Cluster not healthy"

[[workload]]
name = "app"
image = "nixos#app"
replicas = 1
placement = "prefer_idle"
gates = ["deploy_safe"]
"#;

    #[test]
    fn constraints_pass_with_healthy_cluster() {
        let nodes = vec![make_node("a", 20.0), make_node("b", 30.0), make_node("c", 40.0)];
        let orch = Orchestrator::from_px_str(PX, nodes).unwrap();
        let results = orch.evaluate_constraints();
        assert!(results.iter().all(|r| r.passed));
    }

    #[test]
    fn constraints_fail_with_too_few_nodes() {
        let nodes = vec![make_node("a", 20.0)];
        let orch = Orchestrator::from_px_str(PX, nodes).unwrap();
        let results = orch.evaluate_constraints();
        let node_req = results.iter().find(|r| r.constraint_name == "minimum_nodes").unwrap();
        assert!(!node_req.passed);
    }

    #[test]
    fn constraints_fail_with_high_cpu() {
        let nodes = vec![make_node("a", 90.0), make_node("b", 95.0), make_node("c", 85.0)];
        let orch = Orchestrator::from_px_str(PX, nodes).unwrap();
        let results = orch.evaluate_constraints();
        let deploy = results.iter().find(|r| r.constraint_name == "deploy_safe").unwrap();
        assert!(!deploy.passed);
    }

    #[test]
    fn reconcile_blocks_when_constraint_fails() {
        let nodes = vec![make_node("a", 20.0)]; // only 1 node, need 3
        let orch = Orchestrator::from_px_str(PX, nodes).unwrap();
        let result = orch.reconcile();
        assert!(result.assignments.is_empty());
        assert!(!result.blocked.is_empty());
    }

    #[test]
    fn reconcile_succeeds_when_healthy() {
        let nodes = vec![make_node("a", 20.0), make_node("b", 30.0), make_node("c", 40.0)];
        let orch = Orchestrator::from_px_str(PX, nodes).unwrap();
        let result = orch.reconcile();
        assert_eq!(result.assignments.len(), 1);
        assert!(result.blocked.is_empty());
    }
}
