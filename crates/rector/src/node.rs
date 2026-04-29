use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterNode {
    pub id: String,
    pub hostname: String,
    pub addresses: Vec<String>,
    pub capabilities: NodeCapabilities,
    pub status: NodeStatus,
    pub workloads: Vec<String>,
    pub last_seen: u64,
    /// Current CPU utilization 0.0–100.0
    pub cpu_usage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub cpu_cores: u32,
    pub memory_total_mb: u64,
    pub gpu: bool,
    pub gpu_model: Option<String>,
    pub os: String,
    pub arch: String,
    pub is_idle: bool,
    pub bitnet_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeStatus {
    Online,
    Offline,
    Draining,
    Maintenance,
}

impl ClusterNode {
    pub fn is_schedulable(&self) -> bool {
        self.status == NodeStatus::Online
    }

    pub fn available_cpu(&self) -> f64 {
        self.capabilities.cpu_cores as f64 * (1.0 - self.cpu_usage / 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_node(id: &str, cpu_usage: f64, idle: bool) -> ClusterNode {
        ClusterNode {
            id: id.to_string(),
            hostname: format!("{id}.local"),
            addresses: vec!["10.0.0.1".into()],
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
            last_seen: 1700000000,
            cpu_usage,
        }
    }

    #[test]
    fn schedulable_online() {
        let n = test_node("a", 50.0, false);
        assert!(n.is_schedulable());
    }

    #[test]
    fn not_schedulable_draining() {
        let mut n = test_node("b", 10.0, false);
        n.status = NodeStatus::Draining;
        assert!(!n.is_schedulable());
    }

    #[test]
    fn serialization_roundtrip() {
        let n = test_node("c", 25.0, true);
        let json = serde_json::to_string(&n).unwrap();
        let back: ClusterNode = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "c");
        assert!(back.capabilities.is_idle);
    }
}
