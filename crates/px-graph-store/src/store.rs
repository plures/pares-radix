//! PluresDB IO boundary and graph read API.

use std::path::Path;
use std::sync::Arc;

use pluresdb::{CrdtStore, MemoryStorage, SledStorage, StorageEngine};
use px_repo_model::exact_artifact::ExactArtifactProcedure;
use px_repo_model::identity::{BlobHash, ContentHash, LogicalChangeId, ProcedureId, ProcedureRevisionHash, RevisionId};
use px_repo_model::merkle::procedure_root;
use px_repo_model::schema::{Artifact, Blob, LogicalChange, Procedure, ProcedureRevisionContent, Ref, RevisionContent};

use crate::edge::{EdgeKind, EdgeRecord};
use crate::error::GraphStoreError;
use crate::node::{BlobRecord, EmptyDirectoryRecord, GraphNode, NodeKey, StoredNode};

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

    /// Persists a `ProcedureRevision` detail record (graph-schema.md §2.7)
    /// under its own content-derived `ProcedureRevisionHash`. This is
    /// distinct from `put_revision`, which persists the M0 `Revision` entity
    /// (§2.2); a `Procedure`'s `current_revision` field points at records
    /// stored via this method.
    pub fn put_procedure_revision(
        &self,
        hash: ProcedureRevisionHash,
        content: ProcedureRevisionContent,
    ) -> Result<(), GraphStoreError> {
        self.put_node(
            NodeKey::procedure_revision(hash),
            GraphNode::ProcedureRevision(content),
        )
    }

    /// Persists an M1 `ExactArtifactProcedure` metadata record
    /// (graph-schema.md §2.6's `exact_artifact` `ProcedureKind`), keyed by
    /// its owning procedure id.
    pub fn put_exact_artifact(
        &self,
        artifact: ExactArtifactProcedure,
    ) -> Result<(), GraphStoreError> {
        let key = NodeKey::exact_artifact(artifact.procedure_id);
        self.put_node(key, GraphNode::ExactArtifact(artifact))
    }

    /// Persists a projection-facing `Artifact` record (graph-schema.md §2.9).
    pub fn put_artifact(&self, artifact: Artifact) -> Result<(), GraphStoreError> {
        let key = NodeKey::artifact(artifact.owning_procedure, artifact.blob);
        self.put_node(key, GraphNode::Artifact(artifact))
    }

    /// Persists a `LogicalChange` entity (graph-schema.md §2.3).
    pub fn put_logical_change(&self, change: LogicalChange) -> Result<(), GraphStoreError> {
        let key = NodeKey::logical_change(change.id);
        self.put_node(key, GraphNode::LogicalChange(change))
    }

    /// Persists a mutable `Ref` (graph-schema.md §2.5), scoped to the
    /// repository that owns it, and records the `Repository HAS_REF Ref`
    /// edge.
    pub fn put_ref(&self, repository_name: &str, reference: Ref) -> Result<(), GraphStoreError> {
        let key = NodeKey::reference(repository_name, &reference.name);
        self.put_edge(EdgeRecord::new(
            NodeKey::repository(repository_name),
            EdgeKind::RepositoryHasRef,
            key.clone(),
        ))?;
        self.put_node(key, GraphNode::Ref(reference))
    }

    /// Persists a `Blob`'s raw bytes (graph-schema.md §2.10) under its own
    /// content-derived `BlobHash`, recomputed from the given bytes so a
    /// caller can never persist content under a mismatched key.
    pub fn put_blob(&self, blob: Blob) -> Result<BlobHash, GraphStoreError> {
        let hash = blob.hash();
        let key = NodeKey::blob(hash);
        self.put_node(key, GraphNode::Blob(BlobRecord { content: blob.content }))?;
        Ok(hash)
    }

    /// Persists an M1-imported empty directory bookkeeping fact, scoped to
    /// the repository whose tree was imported, and records the
    /// `RepositoryContainsProcedure`-style containment edge for it.
    pub fn put_empty_directory(
        &self,
        repository_name: &str,
        path: &str,
    ) -> Result<(), GraphStoreError> {
        let key = NodeKey::empty_directory(repository_name, path);
        self.put_edge(EdgeRecord::new(
            NodeKey::repository(repository_name),
            EdgeKind::RepositoryContainsEmptyDirectory,
            key.clone(),
        ))?;
        self.put_node(
            key,
            GraphNode::EmptyDirectory(EmptyDirectoryRecord {
                repository_name: repository_name.to_owned(),
                path: path.to_owned(),
            }),
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
            Some(_) | None => Ok(None),
        }
    }

    pub fn get_revision(&self, id: RevisionId) -> Result<Option<RevisionContent>, GraphStoreError> {
        match self.get_node(&NodeKey::revision(id))? {
            Some(GraphNode::Revision(revision)) => Ok(Some(revision)),
            Some(_) | None => Ok(None),
        }
    }

    pub fn get_procedure_revision(
        &self,
        hash: ProcedureRevisionHash,
    ) -> Result<Option<ProcedureRevisionContent>, GraphStoreError> {
        match self.get_node(&NodeKey::procedure_revision(hash))? {
            Some(GraphNode::ProcedureRevision(content)) => Ok(Some(content)),
            Some(_) | None => Ok(None),
        }
    }

    pub fn get_exact_artifact(
        &self,
        procedure_id: ProcedureId,
    ) -> Result<Option<ExactArtifactProcedure>, GraphStoreError> {
        match self.get_node(&NodeKey::exact_artifact(procedure_id))? {
            Some(GraphNode::ExactArtifact(artifact)) => Ok(Some(artifact)),
            Some(_) | None => Ok(None),
        }
    }

    pub fn get_artifact(
        &self,
        owning_procedure: ProcedureId,
        blob: BlobHash,
    ) -> Result<Option<Artifact>, GraphStoreError> {
        match self.get_node(&NodeKey::artifact(owning_procedure, blob))? {
            Some(GraphNode::Artifact(artifact)) => Ok(Some(artifact)),
            Some(_) | None => Ok(None),
        }
    }

    pub fn get_logical_change(
        &self,
        id: LogicalChangeId,
    ) -> Result<Option<LogicalChange>, GraphStoreError> {
        match self.get_node(&NodeKey::logical_change(id))? {
            Some(GraphNode::LogicalChange(change)) => Ok(Some(change)),
            Some(_) | None => Ok(None),
        }
    }

    pub fn get_ref(
        &self,
        repository_name: &str,
        ref_name: &str,
    ) -> Result<Option<Ref>, GraphStoreError> {
        match self.get_node(&NodeKey::reference(repository_name, ref_name))? {
            Some(GraphNode::Ref(reference)) => Ok(Some(reference)),
            Some(_) | None => Ok(None),
        }
    }

    pub fn get_blob(&self, hash: BlobHash) -> Result<Option<Blob>, GraphStoreError> {
        match self.get_node(&NodeKey::blob(hash))? {
            Some(GraphNode::Blob(record)) => Ok(Some(Blob { content: record.content })),
            Some(_) | None => Ok(None),
        }
    }

    pub fn get_empty_directories(
        &self,
        repository_name: &str,
    ) -> Result<Vec<String>, GraphStoreError> {
        let children = self.get_children(
            &NodeKey::repository(repository_name),
            EdgeKind::RepositoryContainsEmptyDirectory,
        )?;
        let mut paths = Vec::with_capacity(children.len());
        for key in children {
            if let Some(GraphNode::EmptyDirectory(record)) = self.get_node(&key)? {
                paths.push(record.path);
            }
        }
        paths.sort();
        Ok(paths)
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
