use crate::node::{ClusterNode, NodeStatus};
use crate::px_parser::{PlacementStrategy, PxWorkload, ReplicaSpec};

#[derive(Debug, Clone)]
pub struct NodeAssignment {
    pub node_id: String,
    pub workload_name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("no schedulable nodes available")]
    NoNodes,
    #[error("insufficient resources on any node")]
    InsufficientResources,
    #[error("placement strategy not yet implemented: {0}")]
    Unimplemented(String),
}

pub struct WorkloadScheduler {
    nodes: Vec<ClusterNode>,
}

impl WorkloadScheduler {
    pub fn new(nodes: Vec<ClusterNode>) -> Self {
        Self { nodes }
    }

    pub fn schedulable_nodes(&self) -> Vec<&ClusterNode> {
        self.nodes.iter().filter(|n| n.is_schedulable()).collect()
    }

    pub fn schedule(&self, workload: &PxWorkload) -> Result<Vec<NodeAssignment>, ScheduleError> {
        let placement = &workload.placement;
        match placement {
            PlacementStrategy::PreferIdle => self.schedule_prefer_idle(workload),
            PlacementStrategy::Everywhere => self.schedule_everywhere(workload),
            PlacementStrategy::Spread => self.schedule_spread(workload),
            PlacementStrategy::Colocate(w) => {
                Err(ScheduleError::Unimplemented(format!("colocate({w})")))
            }
            PlacementStrategy::Pin(n) => Err(ScheduleError::Unimplemented(format!("pin({n})"))),
        }
    }

    fn schedule_prefer_idle(&self, workload: &PxWorkload) -> Result<Vec<NodeAssignment>, ScheduleError> {
        let mut candidates: Vec<&ClusterNode> = self.schedulable_nodes();
        if candidates.is_empty() {
            return Err(ScheduleError::NoNodes);
        }

        // Prefer idle desktops, then sort by lowest CPU usage
        candidates.sort_by(|a, b| {
            b.capabilities
                .is_idle
                .cmp(&a.capabilities.is_idle)
                .then_with(|| a.cpu_usage.partial_cmp(&b.cpu_usage).unwrap())
        });

        let count = match &workload.replicas {
            ReplicaSpec::Count(n) => *n as usize,
            ReplicaSpec::Named(_) => candidates.len(), // "all"
        };

        let assignments: Vec<NodeAssignment> = candidates
            .iter()
            .take(count)
            .map(|n| NodeAssignment {
                node_id: n.id.clone(),
                workload_name: workload.name.clone(),
            })
            .collect();

        if assignments.is_empty() {
            Err(ScheduleError::NoNodes)
        } else {
            Ok(assignments)
        }
    }

    fn schedule_everywhere(&self, workload: &PxWorkload) -> Result<Vec<NodeAssignment>, ScheduleError> {
        let nodes = self.schedulable_nodes();
        if nodes.is_empty() {
            return Err(ScheduleError::NoNodes);
        }
        Ok(nodes
            .iter()
            .map(|n| NodeAssignment {
                node_id: n.id.clone(),
                workload_name: workload.name.clone(),
            })
            .collect())
    }

    fn schedule_spread(&self, workload: &PxWorkload) -> Result<Vec<NodeAssignment>, ScheduleError> {
        // Spread = round-robin across nodes sorted by fewest existing workloads
        let mut candidates: Vec<&ClusterNode> = self.schedulable_nodes();
        if candidates.is_empty() {
            return Err(ScheduleError::NoNodes);
        }
        candidates.sort_by_key(|n| n.workloads.len());

        let count = match &workload.replicas {
            ReplicaSpec::Count(n) => *n as usize,
            ReplicaSpec::Named(_) => candidates.len(),
        };

        Ok(candidates
            .iter()
            .take(count)
            .map(|n| NodeAssignment {
                node_id: n.id.clone(),
                workload_name: workload.name.clone(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{ClusterNode, NodeCapabilities, NodeStatus};
    use crate::px_parser::*;

    fn make_node(id: &str, cpu: f64, idle: bool) -> ClusterNode {
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
                is_idle: idle,
                bitnet_available: false,
            },
            status: NodeStatus::Online,
            workloads: vec![],
            last_seen: 0,
            cpu_usage: cpu,
        }
    }

    fn make_workload(name: &str, placement: PlacementStrategy, replicas: ReplicaSpec) -> PxWorkload {
        PxWorkload {
            name: name.into(),
            image: "nixos#test".into(),
            replicas,
            placement,
            resources: ResourceSpec::default(),
            health: None,
            gates: vec![],
            on_failure: FailurePolicy::default(),
        }
    }

    #[test]
    fn prefer_idle_picks_lowest_cpu() {
        let nodes = vec![
            make_node("busy", 90.0, false),
            make_node("idle", 10.0, false),
            make_node("medium", 50.0, false),
        ];
        let sched = WorkloadScheduler::new(nodes);
        let w = make_workload("test", PlacementStrategy::PreferIdle, ReplicaSpec::Count(1));
        let assignments = sched.schedule(&w).unwrap();
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0].node_id, "idle");
    }

    #[test]
    fn prefer_idle_prefers_idle_desktops() {
        let nodes = vec![
            make_node("server", 10.0, false),
            make_node("desktop", 20.0, true), // idle desktop
        ];
        let sched = WorkloadScheduler::new(nodes);
        let w = make_workload("test", PlacementStrategy::PreferIdle, ReplicaSpec::Count(1));
        let assignments = sched.schedule(&w).unwrap();
        assert_eq!(assignments[0].node_id, "desktop");
    }

    #[test]
    fn everywhere_assigns_all_nodes() {
        let nodes = vec![
            make_node("a", 10.0, false),
            make_node("b", 20.0, false),
            make_node("c", 30.0, false),
        ];
        let sched = WorkloadScheduler::new(nodes);
        let w = make_workload("test", PlacementStrategy::Everywhere, ReplicaSpec::Named("all".into()));
        let assignments = sched.schedule(&w).unwrap();
        assert_eq!(assignments.len(), 3);
    }

    #[test]
    fn no_nodes_errors() {
        let sched = WorkloadScheduler::new(vec![]);
        let w = make_workload("test", PlacementStrategy::PreferIdle, ReplicaSpec::Count(1));
        assert!(sched.schedule(&w).is_err());
    }

    #[test]
    fn draining_nodes_excluded() {
        let mut n = make_node("drain", 5.0, false);
        n.status = NodeStatus::Draining;
        let sched = WorkloadScheduler::new(vec![n]);
        let w = make_workload("test", PlacementStrategy::Everywhere, ReplicaSpec::Count(1));
        assert!(sched.schedule(&w).is_err());
    }
}
