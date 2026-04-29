//! Node discovery for cluster peers.
//!
//! Provides both a static in-memory implementation for tests and a
//! PluresDB-backed discovery that uses the CRDT store (synced via
//! Hyperswarm) to announce and discover cluster nodes.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::node::{ClusterNode, NodeCapabilities};

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("discovery unavailable: {0}")]
    Unavailable(String),
}

/// Static discovery — returns a fixed set of nodes. Useful for tests and single-node.
pub struct StaticDiscovery {
    nodes: Vec<ClusterNode>,
}

impl StaticDiscovery {
    pub fn new(nodes: Vec<ClusterNode>) -> Self {
        Self { nodes }
    }

    pub fn discover(&self) -> Vec<ClusterNode> {
        self.nodes.clone()
    }
}

// ── PluresDB-backed discovery ─────────────────────────────────────────

/// Trait abstracting the CRDT store operations needed for discovery.
/// This avoids a hard dependency on a specific PluresDB crate from rector.
pub trait DiscoveryStore: Send + Sync {
    /// Write a key-value pair into the store.
    fn put(&self, key: &str, author: &str, value: Value);
    /// Read a value by key.
    fn get(&self, key: &str) -> Option<Value>;
    /// List all keys matching a prefix.
    fn keys_with_prefix(&self, prefix: &str) -> Vec<String>;
}

/// Discovery backed by PluresDB's CRDT store.
///
/// Each node announces itself by writing its [`ClusterNode`] as a JSON
/// value under `cluster:node:<id>`. Discovery reads all such entries and
/// filters out nodes not seen within the staleness window.
pub struct PluresDbDiscovery {
    store: Arc<dyn DiscoveryStore>,
    local_node: ClusterNode,
}

/// Nodes not seen within this many seconds are considered stale.
const STALE_THRESHOLD_SECS: u64 = 300; // 5 minutes
/// Announcement interval in seconds.
const ANNOUNCE_INTERVAL_SECS: u64 = 30;

impl PluresDbDiscovery {
    pub fn new(store: Arc<dyn DiscoveryStore>, local_node: ClusterNode) -> Self {
        Self { store, local_node }
    }

    /// Return a reference to the local node.
    pub fn local_node(&self) -> &ClusterNode {
        &self.local_node
    }

    /// Announce this node to the cluster by writing to PluresDB.
    pub fn announce(&mut self) {
        self.local_node.last_seen = now_epoch_secs();
        let key = format!("cluster:node:{}", self.local_node.id);
        let value = serde_json::to_value(&self.local_node).unwrap();
        self.store.put(&key, "rector", value);
    }

    /// Discover all nodes by reading PluresDB.
    /// Filters out nodes whose `last_seen` is older than [`STALE_THRESHOLD_SECS`].
    pub fn discover(&self) -> Vec<ClusterNode> {
        let now = now_epoch_secs();
        let keys = self.store.keys_with_prefix("cluster:node:");
        let mut nodes = Vec::new();
        for key in keys {
            if let Some(value) = self.store.get(&key) {
                if let Ok(node) = serde_json::from_value::<ClusterNode>(value) {
                    if now.saturating_sub(node.last_seen) <= STALE_THRESHOLD_SECS {
                        nodes.push(node);
                    }
                }
            }
        }
        nodes
    }

    /// Start periodic announcement. Runs until `shutdown` receives `true`.
    pub async fn run_announcer(&mut self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(ANNOUNCE_INTERVAL_SECS));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.announce();
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    }

    /// Detect local node capabilities by inspecting the host system.
    pub fn detect_local_capabilities() -> NodeCapabilities {
        NodeCapabilities {
            cpu_cores: num_cpus::get() as u32,
            memory_total_mb: read_meminfo_total_mb(),
            gpu: std::path::Path::new("/dev/dri/renderD128").exists(),
            gpu_model: detect_gpu_model(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            is_idle: detect_idle_state(),
            bitnet_available: false, // set from config
        }
    }
}

// ── System info helpers ───────────────────────────────────────────────

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn read_meminfo_total_mb() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .map(|l| l.to_string())
        })
        .and_then(|l| l.split_whitespace().nth(1).map(|s| s.to_string()))
        .and_then(|v| v.parse::<u64>().ok())
        .map(|kb| kb / 1024)
        .unwrap_or(0)
}

fn detect_gpu_model() -> Option<String> {
    // Try lspci first
    if let Ok(output) = std::process::Command::new("lspci").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("VGA") || line.contains("3D controller") || line.contains("Display") {
                // Extract the model name after the last colon
                if let Some(model) = line.split(':').last() {
                    let model = model.trim();
                    if !model.is_empty() {
                        return Some(model.to_string());
                    }
                }
            }
        }
    }
    None
}

fn detect_idle_state() -> bool {
    // On Linux, check if there are active user sessions via /run/user/
    // If loginctl is available, check for "active" sessions.
    if let Ok(output) = std::process::Command::new("loginctl")
        .args(["list-sessions", "--no-legend"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // No active sessions = idle
        return stdout.trim().is_empty();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeStatus;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory store for testing.
    struct MemStore {
        data: Mutex<HashMap<String, Value>>,
    }

    impl MemStore {
        fn new() -> Self {
            Self {
                data: Mutex::new(HashMap::new()),
            }
        }
    }

    impl DiscoveryStore for MemStore {
        fn put(&self, key: &str, _author: &str, value: Value) {
            self.data.lock().unwrap().insert(key.to_string(), value);
        }

        fn get(&self, key: &str) -> Option<Value> {
            self.data.lock().unwrap().get(key).cloned()
        }

        fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
            self.data
                .lock()
                .unwrap()
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect()
        }
    }

    fn test_node(id: &str) -> ClusterNode {
        ClusterNode {
            id: id.to_string(),
            hostname: format!("{id}.local"),
            addresses: vec!["10.0.0.1".into()],
            capabilities: NodeCapabilities {
                cpu_cores: 4,
                memory_total_mb: 8192,
                gpu: false,
                gpu_model: None,
                os: "linux".into(),
                arch: "x86_64".into(),
                is_idle: false,
                bitnet_available: false,
            },
            status: NodeStatus::Online,
            workloads: vec![],
            last_seen: now_epoch_secs(),
            cpu_usage: 25.0,
        }
    }

    #[test]
    fn announce_and_discover() {
        let store: Arc<dyn DiscoveryStore> = Arc::new(MemStore::new());
        let node = test_node("node-1");
        let mut discovery = PluresDbDiscovery::new(Arc::clone(&store), node);

        discovery.announce();
        let nodes = discovery.discover();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "node-1");
    }

    #[test]
    fn stale_nodes_filtered() {
        let store: Arc<dyn DiscoveryStore> = Arc::new(MemStore::new());
        let mut node = test_node("stale");
        node.last_seen = 0; // epoch = very stale
        store.put(
            "cluster:node:stale",
            "rector",
            serde_json::to_value(&node).unwrap(),
        );

        let fresh = test_node("fresh");
        let mut discovery = PluresDbDiscovery::new(Arc::clone(&store), fresh);
        discovery.announce();

        let nodes = discovery.discover();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "fresh");
    }

    #[test]
    fn static_discovery_returns_configured_nodes() {
        let nodes = vec![test_node("a"), test_node("b")];
        let disc = StaticDiscovery::new(nodes);
        assert_eq!(disc.discover().len(), 2);
    }
}
