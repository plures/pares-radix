//! PluresDB IO boundary and graph read API.

use std::path::Path;
use std::sync::Arc;

use pluresdb::{CrdtStore, MemoryStorage, SledStorage, StorageEngine};
use px_repo_model::identity::{ContentHash, ProcedureId, RevisionId};
use px_repo_model::merkle::procedure_root;
use px_repo_model::schema::{Procedure, RevisionContent};

use crate::edge::{EdgeKind, EdgeRecord};
use crate::error::GraphStoreError;
use crate::node::{GraphNode, NodeKey, StoredNode};

const ACTOR_ID: &str = "px-graph-store";
const NODE_PREFIX: &str = "px_graph_node:v1:";

/// Procedure graph persistence backed exclusively by PluresDB.
pub struct GraphStore {
    store: CrdtStore,
}

impl GraphStore {
    /// Opens durable PluresDB/Sled storage. `SledStorage` is PluresDB's
    /// persistence engine; this crate does not create its own data files.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, GraphStoreError> {
        let display_path = path.as_ref().display().to_string();
        let storage: Arc<dyn StorageEngine> = Arc::new(SledStorage::open(path).map_err(
            |error| GraphStoreError::Open {
                path: display_path,
                message: error.to_string(),
            },
        )?);
        Ok(Self {
            store: CrdtStore::default().with_persistence(storage),
        })
    }

    /// Uses PluresDB's in-memory storage engine. This exists solely for tests;
    /// production persistence uses [`Self::open`].
    pub fn in_memory() -> Self {
        let storage: Arc<dyn StorageEngine> = Arc::new(MemoryStorage::default());
        Self {
            store: CrdtStore::default().with_persistence(storage),
        }
    }

    /// Persists one procedure and its current revision relation as a typed
    /// PluresDB node. The caller persists revision content separately.
    pub fn put_procedure(&self, procedure: Procedure) -> Result<(), GraphStoreError> {
        let key = NodeKey::procedure(procedure.id);
        self.put_node(key, GraphNode::Procedure(procedure))
    }

    /// Persists revision content under its already canonical, content-derived
    /// `RevisionId`. It additionally persists each parent → revision edge.
    pub fn put_revision(
        &self,
        revision_id: RevisionId,
        revision: RevisionContent,
    ) -> Result<(), GraphStoreError> {
        for parent in &revision.parents {
            self.put_edge(EdgeRecord::new(
                NodeKey::revision(*parent),
                EdgeKind::RevisionParentOf,
                NodeKey::revision(revision_id),
            ))?;
        }
        self.put_node(
            NodeKey::revision(revision_id),
            GraphNode::Revision(revision),
        )
    }

    /// Explicitly persists a graph edge in PluresDB.
    pub fn put_edge(&self, edge: EdgeRecord) -> Result<(), GraphStoreError> {
        let key = edge.storage_key();
        let value = serde_json::to_value(edge).map_err(|source| GraphStoreError::Serialize {
            key: key.clone(),
            source,
        })?;
        self.store.put(&key, ACTOR_ID, value);
        Ok(())
    }

    /// Gets a typed graph node by its domain key from PluresDB.
    pub fn get_node(&self, key: &NodeKey) -> Result<Option<GraphNode>, GraphStoreError> {
        let storage_key = node_storage_key(key);
        self.store
            .get(&storage_key)
            .map(|record| decode_node(&storage_key, record.data).map(|stored| stored.node))
            .transpose()
    }

    pub fn get_procedure(&self, id: ProcedureId) -> Result<Option<Procedure>, GraphStoreError> {
        match self.get_node(&NodeKey::procedure(id))? {
            Some(GraphNode::Procedure(procedure)) => Ok(Some(procedure)),
            Some(GraphNode::Revision(_)) | None => Ok(None),
        }
    }

    pub fn get_revision(&self, id: RevisionId) -> Result<Option<RevisionContent>, GraphStoreError> {
        match self.get_node(&NodeKey::revision(id))? {
            Some(GraphNode::Revision(revision)) => Ok(Some(revision)),
            Some(GraphNode::Procedure(_)) | None => Ok(None),
        }
    }

    /// Returns direct children of `from` for an edge kind. The selection rule
    /// is specified in `praxis/procedures/procedure-graph-queries.px`; this
    /// method performs only the PluresDB scan/deserialization required to
    /// apply that rule to persisted edge records.
    pub fn get_children(
        &self,
        from: &NodeKey,
        kind: EdgeKind,
    ) -> Result<Vec<NodeKey>, GraphStoreError> {
        let mut children = self
            .all_edges()?
            .into_iter()
            .filter(|edge| edge.from == *from && edge.kind == kind)
            .map(|edge| edge.to)
            .collect::<Vec<_>>();
        children.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        Ok(children)
    }

    /// Gets the historical procedure root asserted in a persisted revision.
    /// This is the M2 "at a point in time" query: it reads the immutable
    /// revision's recorded root rather than recalculating from current state.
    pub fn merkle_root_at_revision(
        &self,
        revision_id: RevisionId,
    ) -> Result<Option<ContentHash>, GraphStoreError> {
        Ok(self
            .get_revision(revision_id)?
            .map(|revision| revision.procedure_root))
    }

    /// Recomputes the current procedure root over procedures stored in
    /// PluresDB, delegating all canonical ordering/encoding/BLAKE3 work to
    /// M0's `px_repo_model::merkle::procedure_root`.
    pub fn current_procedure_root(&self) -> Result<ContentHash, GraphStoreError> {
        let mut entries = Vec::new();
        for record in self.store.list() {
            let key = record.id.to_string();
            if !key.starts_with(NODE_PREFIX) {
                continue;
            }
            let node = decode_node(&key, record.data)?;
            if let GraphNode::Procedure(procedure) = node.node {
                entries.push((procedure.id, procedure.current_revision));
            }
        }
        procedure_root(entries).map_err(GraphStoreError::from)
    }

    fn put_node(&self, key: NodeKey, node: GraphNode) -> Result<(), GraphStoreError> {
        let storage_key = node_storage_key(&key);
        let value = serde_json::to_value(StoredNode::new(key, node)).map_err(|source| {
            GraphStoreError::Serialize {
                key: storage_key.clone(),
                source,
            }
        })?;
        self.store.put(&storage_key, ACTOR_ID, value);
        Ok(())
    }

    fn all_edges(&self) -> Result<Vec<EdgeRecord>, GraphStoreError> {
        self.store
            .list()
            .into_iter()
            .filter_map(|record| {
                let key = record.id.to_string();
                key.starts_with("px_graph_edge:v1:").then(|| {
                    serde_json::from_value(record.data)
                        .map_err(|source| GraphStoreError::Deserialize { key, source })
                })
            })
            .collect()
    }
}

fn node_storage_key(key: &NodeKey) -> String {
    format!("{NODE_PREFIX}{}", key.as_str())
}

fn decode_node(storage_key: &str, data: serde_json::Value) -> Result<StoredNode, GraphStoreError> {
    let node = serde_json::from_value(data).map_err(|source| GraphStoreError::Deserialize {
        key: storage_key.to_string(),
        source,
    })?;
    Ok(node)
}
