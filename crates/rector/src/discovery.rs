//! Node discovery via Hyperswarm (stub).
//!
//! Real implementation will use Hyperswarm DHT to discover cluster peers.
//! For now this provides a static in-memory implementation for tests.

use crate::node::ClusterNode;

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
