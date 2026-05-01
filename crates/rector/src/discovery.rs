//! Node discovery for cluster peers.
//!
//! Provides both a static in-memory implementation for tests and a
//! PluresDB-backed discovery that uses the CRDT store (synced via
//! Hyperswarm) to announce and discover cluster nodes.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::node::{ClusterNode, NodeCapabilities, NodeStatus};

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
                if let Some(model) = line.split(':').next_back() {
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

// ── Direct peer + LAN multicast discovery ─────────────────────────────

/// A parsed peer address (host:port).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAddress {
    pub address: String,
    pub port: u16,
}

impl PeerAddress {
    /// Parse "10.0.0.5:7700", "[::1]:7700", or "host.local:7700".
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if let Some(idx) = s.rfind(':') {
            let port = s[idx + 1..].parse().ok()?;
            let addr = s[..idx].trim_matches(|c| c == '[' || c == ']').to_string();
            if addr.is_empty() {
                return None;
            }
            Some(Self { address: addr, port })
        } else {
            None
        }
    }
}

/// Discovery modes supported by the multi-mode discovery engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoveryMode {
    /// Static list of known peer addresses.
    Direct(Vec<PeerAddress>),
    /// LAN multicast announce/listen.
    Lan { multicast_group: String, port: u16 },
    /// PluresDB CRDT store (existing).
    PluresDb,
}

/// Multi-mode discovery: combines Direct, LAN, and PluresDB discovery.
pub struct MultiDiscovery {
    modes: Vec<DiscoveryMode>,
    store: Arc<dyn DiscoveryStore>,
    local_node: ClusterNode,
}

impl MultiDiscovery {
    pub fn new(store: Arc<dyn DiscoveryStore>, local_node: ClusterNode) -> Self {
        Self {
            modes: vec![DiscoveryMode::PluresDb],
            store,
            local_node,
        }
    }

    pub fn add_direct_peers(&mut self, peers: Vec<PeerAddress>) {
        self.modes.push(DiscoveryMode::Direct(peers));
    }

    pub fn add_lan(&mut self, group: &str, port: u16) {
        self.modes.push(DiscoveryMode::Lan {
            multicast_group: group.into(),
            port,
        });
    }

    /// Discover nodes from all configured modes, deduplicating by node ID.
    pub fn discover_all(&self) -> Vec<ClusterNode> {
        let mut nodes = Vec::new();
        let mut seen = HashSet::new();

        for mode in &self.modes {
            let found = match mode {
                DiscoveryMode::Direct(peers) => self.discover_direct(peers),
                DiscoveryMode::Lan { multicast_group, port } => {
                    self.discover_lan(multicast_group, *port)
                }
                DiscoveryMode::PluresDb => self.discover_pluresdb(),
            };
            for node in found {
                if seen.insert(node.id.clone()) {
                    nodes.push(node);
                }
            }
        }
        nodes
    }

    fn discover_direct(&self, peers: &[PeerAddress]) -> Vec<ClusterNode> {
        peers
            .iter()
            .map(|p| {
                // Try to find in store first
                let key = format!("cluster:node:direct:{}", p.address);
                if let Some(existing) = self.store.get(&key) {
                    if let Ok(node) = serde_json::from_value::<ClusterNode>(existing) {
                        return node;
                    }
                }
                // Create minimal node from address
                ClusterNode {
                    id: format!("direct-{}-{}", p.address, p.port),
                    hostname: p.address.clone(),
                    addresses: vec![format!("{}:{}", p.address, p.port)],
                    capabilities: NodeCapabilities::default(),
                    status: NodeStatus::Online,
                    workloads: vec![],
                    last_seen: now_epoch_secs(),
                    cpu_usage: 0.0,
                }
            })
            .collect()
    }

    fn discover_lan(&self, group: &str, port: u16) -> Vec<ClusterNode> {
        use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
        use std::time::Duration;

        const MAGIC: &[u8; 4] = b"PLRS";

        let multicast_addr: Ipv4Addr = group.parse().unwrap_or(Ipv4Addr::new(239, 255, 77, 77));

        // Announce ourselves
        if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
            let payload = serde_json::to_vec(&self.local_node).unwrap_or_default();
            let mut packet = Vec::with_capacity(4 + payload.len());
            packet.extend_from_slice(MAGIC);
            packet.extend_from_slice(&payload);
            let dest = SocketAddrV4::new(multicast_addr, port);
            let _ = socket.send_to(&packet, dest);
        }

        // Listen for others (1 second)
        let mut nodes = Vec::new();
        if let Ok(socket) = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)) {
            let _ = socket.join_multicast_v4(&multicast_addr, &Ipv4Addr::UNSPECIFIED);
            let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));

            let mut buf = [0u8; 65535];
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((len, _)) if len > 4 && &buf[..4] == MAGIC => {
                        if let Ok(node) = serde_json::from_slice::<ClusterNode>(&buf[4..len]) {
                            if node.id != self.local_node.id {
                                nodes.push(node);
                            }
                        }
                    }
                    _ => break,
                }
            }
        }

        nodes
    }

    fn discover_pluresdb(&self) -> Vec<ClusterNode> {
        let now = now_epoch_secs();
        let keys = self.store.keys_with_prefix("cluster:node:");
        keys.into_iter()
            .filter_map(|key| self.store.get(&key))
            .filter_map(|v| serde_json::from_value::<ClusterNode>(v).ok())
            .filter(|n| now.saturating_sub(n.last_seen) <= STALE_THRESHOLD_SECS)
            .collect()
    }

    /// Announce this node to the PluresDB store.
    pub fn announce(&mut self) {
        self.local_node.last_seen = now_epoch_secs();
        let key = format!("cluster:node:{}", self.local_node.id);
        let value = serde_json::to_value(&self.local_node).unwrap();
        self.store.put(&key, "rector", value);
    }
}

/// Run a periodic LAN multicast announcer as a background task.
///
/// Sends a UDP multicast packet containing the local node's JSON representation
/// at the given interval, until the shutdown signal is received.
pub async fn run_lan_announcer(
    local_node: &ClusterNode,
    group: &str,
    port: u16,
    interval: std::time::Duration,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};

    let multicast_addr: Ipv4Addr = group.parse().unwrap_or(Ipv4Addr::new(239, 255, 77, 77));

    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = tokio::time::sleep(interval) => {
                if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
                    let payload = serde_json::to_vec(local_node).unwrap_or_default();
                    let mut packet = Vec::with_capacity(4 + payload.len());
                    packet.extend_from_slice(b"PLRS");
                    packet.extend_from_slice(&payload);
                    let _ = socket.send_to(&packet, SocketAddrV4::new(multicast_addr, port));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    // ── PeerAddress tests ─────────────────────────────────────────────

    #[test]
    fn peer_address_parse_ipv4() {
        let p = PeerAddress::parse("10.0.0.5:7700").unwrap();
        assert_eq!(p.address, "10.0.0.5");
        assert_eq!(p.port, 7700);
    }

    #[test]
    fn peer_address_parse_ipv6() {
        let p = PeerAddress::parse("[::1]:7700").unwrap();
        assert_eq!(p.address, "::1");
        assert_eq!(p.port, 7700);
    }

    #[test]
    fn peer_address_parse_hostname() {
        let p = PeerAddress::parse("host.local:8080").unwrap();
        assert_eq!(p.address, "host.local");
        assert_eq!(p.port, 8080);
    }

    #[test]
    fn peer_address_parse_invalid() {
        assert!(PeerAddress::parse("noport").is_none());
        assert!(PeerAddress::parse(":7700").is_none());
    }

    // ── MultiDiscovery tests ──────────────────────────────────────────

    #[test]
    fn multi_discovery_direct_peers() {
        let store: Arc<dyn DiscoveryStore> = Arc::new(MemStore::new());
        let local = test_node("local");
        let mut md = MultiDiscovery::new(Arc::clone(&store), local);
        md.add_direct_peers(vec![
            PeerAddress { address: "10.0.0.5".into(), port: 7700 },
            PeerAddress { address: "10.0.0.6".into(), port: 7700 },
        ]);
        let nodes = md.discover_all();
        // Should include direct peers (no PluresDB nodes stored)
        assert_eq!(nodes.len(), 2);
        assert!(nodes[0].id.starts_with("direct-"));
    }

    #[test]
    fn multi_discovery_deduplication() {
        let store: Arc<dyn DiscoveryStore> = Arc::new(MemStore::new());
        let local = test_node("local");
        let mut md = MultiDiscovery::new(Arc::clone(&store), local.clone());

        // Announce local via store so PluresDB mode finds it
        md.announce();

        // Also add as direct peer with same id pattern won't collide,
        // but add another peer that IS in the store under same id
        let key = "cluster:node:direct:10.0.0.5";
        let mut dup = test_node("dup-node");
        dup.last_seen = now_epoch_secs();
        store.put(key, "rector", serde_json::to_value(&dup).unwrap());

        md.add_direct_peers(vec![PeerAddress { address: "10.0.0.5".into(), port: 7700 }]);

        let nodes = md.discover_all();
        // PluresDB finds "local", direct finds "dup-node" — both unique
        let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(&"local"));
        assert!(ids.contains(&"dup-node"));
        // No duplicates
        let unique: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len());
    }

    #[test]
    fn multi_discovery_direct_uses_store_entry() {
        let store: Arc<dyn DiscoveryStore> = Arc::new(MemStore::new());
        let local = test_node("local");
        let md = MultiDiscovery::new(Arc::clone(&store), local);

        // Pre-populate store with a rich node entry for this direct peer
        let mut rich_node = test_node("rich-peer");
        rich_node.capabilities.cpu_cores = 32;
        store.put(
            "cluster:node:direct:10.0.0.99",
            "rector",
            serde_json::to_value(&rich_node).unwrap(),
        );

        let results = md.discover_direct(&[PeerAddress { address: "10.0.0.99".into(), port: 7700 }]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "rich-peer");
        assert_eq!(results[0].capabilities.cpu_cores, 32);
    }
}
