//! PluresDB storage adapter for omniscient file nodes.
//!
//! Maps FileNode ↔ PluresDB graph nodes with vector embeddings.

use crate::file_node::FileNode;
use std::collections::HashMap;

/// Storage backend trait for the omniscient index.
pub trait OmniscientStore: Send + Sync {
    /// Store or update a file node.
    fn upsert(&self, node: &FileNode) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Get a file node by path + node_id.
    fn get(
        &self,
        node_id: &str,
        path: &str,
    ) -> Result<Option<FileNode>, Box<dyn std::error::Error + Send + Sync>>;

    /// Delete a file node.
    fn delete(
        &self,
        node_id: &str,
        path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Find nodes with the same content hash (cross-system duplicates).
    fn find_by_hash(
        &self,
        content_hash: &str,
    ) -> Result<Vec<FileNode>, Box<dyn std::error::Error + Send + Sync>>;

    /// Get all nodes that need Pass 2 enrichment (enriched_at is None).
    fn unenriched(
        &self,
        limit: usize,
    ) -> Result<Vec<FileNode>, Box<dyn std::error::Error + Send + Sync>>;

    /// Get stats.
    fn stats(&self) -> Result<StoreStats, Box<dyn std::error::Error + Send + Sync>>;
}

/// Index statistics.
#[derive(Debug, Clone, Default)]
pub struct StoreStats {
    /// Total files indexed
    pub total_files: usize,
    /// Files per node
    pub files_per_node: HashMap<String, usize>,
    /// Files with Pass 2 enrichment complete
    pub enriched_files: usize,
    /// Files awaiting enrichment
    pub pending_enrichment: usize,
    /// Total storage used by vectors
    pub vector_bytes: usize,
    /// Content classes breakdown
    pub by_class: HashMap<String, usize>,
}

/// In-memory store for testing and development.
pub struct MemoryStore {
    nodes: std::sync::RwLock<Vec<FileNode>>,
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            nodes: std::sync::RwLock::new(Vec::new()),
        }
    }
}

impl OmniscientStore for MemoryStore {
    fn upsert(&self, node: &FileNode) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut nodes = self.nodes.write().map_err(|e| format!("lock: {}", e))?;
        // Remove existing entry with same path + node_id
        nodes.retain(|n| !(n.path == node.path && n.node.node_id == node.node.node_id));
        nodes.push(node.clone());
        Ok(())
    }

    fn get(
        &self,
        node_id: &str,
        path: &str,
    ) -> Result<Option<FileNode>, Box<dyn std::error::Error + Send + Sync>> {
        let nodes = self.nodes.read().map_err(|e| format!("lock: {}", e))?;
        Ok(nodes
            .iter()
            .find(|n| n.node.node_id == node_id && n.path == path)
            .cloned())
    }

    fn delete(
        &self,
        node_id: &str,
        path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut nodes = self.nodes.write().map_err(|e| format!("lock: {}", e))?;
        nodes.retain(|n| !(n.path == path && n.node.node_id == node_id));
        Ok(())
    }

    fn find_by_hash(
        &self,
        content_hash: &str,
    ) -> Result<Vec<FileNode>, Box<dyn std::error::Error + Send + Sync>> {
        let nodes = self.nodes.read().map_err(|e| format!("lock: {}", e))?;
        Ok(nodes
            .iter()
            .filter(|n| n.content_hash == content_hash)
            .cloned()
            .collect())
    }

    fn unenriched(
        &self,
        limit: usize,
    ) -> Result<Vec<FileNode>, Box<dyn std::error::Error + Send + Sync>> {
        let nodes = self.nodes.read().map_err(|e| format!("lock: {}", e))?;
        Ok(nodes
            .iter()
            .filter(|n| n.enriched_at.is_none())
            .take(limit)
            .cloned()
            .collect())
    }

    fn stats(&self) -> Result<StoreStats, Box<dyn std::error::Error + Send + Sync>> {
        let nodes = self.nodes.read().map_err(|e| format!("lock: {}", e))?;
        let mut stats = StoreStats {
            total_files: nodes.len(),
            ..Default::default()
        };

        for node in nodes.iter() {
            *stats
                .files_per_node
                .entry(node.node.node_id.clone())
                .or_insert(0) += 1;
            *stats
                .by_class
                .entry(format!("{:?}", node.content_class))
                .or_insert(0) += 1;
            if node.enriched_at.is_some() {
                stats.enriched_files += 1;
            } else {
                stats.pending_enrichment += 1;
            }
        }

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_node::{FileNodeBuilder, NodeIdentity};

    fn make_test_node(name: &str) -> FileNode {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, format!("content of {}", name)).unwrap();
        // Need to keep dir alive, so we leak it for test purposes
        let path_str = path.to_str().unwrap().to_string();
        std::mem::forget(dir);
        FileNodeBuilder::new(&path_str).build_from_fs().unwrap()
    }

    #[test]
    fn test_memory_store_upsert_and_get() {
        let store = MemoryStore::new();
        let node = make_test_node("test.txt");
        let node_id = node.node.node_id.clone();
        let path = node.path.clone();

        store.upsert(&node).unwrap();
        let got = store.get(&node_id, &path).unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().path, path);
    }

    #[test]
    fn test_memory_store_delete() {
        let store = MemoryStore::new();
        let node = make_test_node("delete_me.txt");
        let node_id = node.node.node_id.clone();
        let path = node.path.clone();

        store.upsert(&node).unwrap();
        store.delete(&node_id, &path).unwrap();
        assert!(store.get(&node_id, &path).unwrap().is_none());
    }

    #[test]
    fn test_memory_store_stats() {
        let store = MemoryStore::new();
        store.upsert(&make_test_node("a.txt")).unwrap();
        store.upsert(&make_test_node("b.rs")).unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.pending_enrichment, 2);
        assert_eq!(stats.enriched_files, 0);
    }

    #[test]
    fn test_memory_store_unenriched() {
        let store = MemoryStore::new();
        store.upsert(&make_test_node("x.txt")).unwrap();
        store.upsert(&make_test_node("y.txt")).unwrap();

        let unenriched = store.unenriched(10).unwrap();
        assert_eq!(unenriched.len(), 2);
    }
}
