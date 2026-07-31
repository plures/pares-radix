//! Typed graph node envelopes persisted in the flat PluresDB node keyspace.
//!
//! The payload types are the `px-repo-model` types themselves. The envelope
//! adds only a type discriminator required by storage and query dispatch; it
//! does not duplicate any domain schema fields.
//!
//! M3 extends the vocabulary to the remaining M0 graph entities the M2
//! follow-up note named: `ProcedureRevision` detail records, `Artifact`,
//! `Ref`, and `LogicalChange` (graph-schema.md §2.3/§2.5/§2.7/§2.9), plus the
//! `ExactArtifactProcedure` metadata record the M1 adapter produces
//! (graph-schema.md §2.6's `exact_artifact` `ProcedureKind`). All payload
//! types are reused verbatim from `px_repo_model`; no parallel schema is
//! introduced for any M0-named entity.
//!
//! Two node kinds are NOT M0 domain entities and are called out as such:
//! `Blob` (raw file bytes; graph-schema.md §2.10 defines a `Blob` entity but
//! only for its content-derived identity, not a Rust storage struct - this
//! crate is the first thing that needs to actually keep the bytes somewhere,
//! so `BlobRecord` exists solely as the PluresDB-side byte container) and
//! `EmptyDirectory` (directories are explicitly out of the M0 schema per
//! `px-adapter-exact`'s own `ImportedDirectory` doc comment - this crate
//! persists the same "must remember this dir existed" bookkeeping fact the
//! M1 in-memory `ImportedTree` already carries, so a round trip through
//! PluresDB does not silently drop empty directories).

use px_repo_model::exact_artifact::ExactArtifactProcedure;
use px_repo_model::identity::{
    BlobHash, LogicalChangeId, ProcedureId, ProcedureRevisionHash, RevisionId,
};
use px_repo_model::schema::{
    Artifact, LogicalChange, Procedure, ProcedureRevisionContent, Ref, RevisionContent,
};
use serde::{Deserialize, Serialize};

/// Stable graph key, deliberately distinct from PluresDB's raw node key so
/// domain identity and storage namespace cannot be conflated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeKey(String);

impl NodeKey {
    /// Addresses the schema's externally named repository root. Repositories
    /// intentionally have no content-derived graph identity (graph-schema
    /// §2.1), so this is an edge endpoint rather than a stored `GraphNode`.
    pub fn repository(name: impl AsRef<str>) -> Self {
        Self(format!("repository:{}", name.as_ref()))
    }

    pub fn procedure(id: ProcedureId) -> Self {
        Self(format!("procedure:{}", id.to_canonical_text()))
    }

    pub fn revision(id: RevisionId) -> Self {
        Self(format!("revision:{}", id.to_canonical_text()))
    }

    /// Addresses a `ProcedureRevision` detail record (graph-schema.md §2.7)
    /// by its own content-derived `ProcedureRevisionHash`, distinct from the
    /// `revision:` key above, which addresses the M0 `Revision` entity
    /// (graph-schema.md §2.2) - the two are different entities that share
    /// only a similar English name.
    pub fn procedure_revision(hash: ProcedureRevisionHash) -> Self {
        Self(format!("procedure_revision:{}", hash.to_canonical_text()))
    }

    /// Addresses an `Artifact` projection-facing record (graph-schema.md
    /// §2.9). An `Artifact` has no identity of its own - it is identified by
    /// the combination of its owning `Procedure` and the `Blob` it
    /// references (§2.9: "identified by the combination of its owning
    /// `Procedure` and its `PROJECTS_TO Path` relationship") - so the key is
    /// composed from both rather than invented as a new identity.
    pub fn artifact(owning_procedure: ProcedureId, blob: BlobHash) -> Self {
        Self(format!(
            "artifact:{}:{}",
            owning_procedure.to_canonical_text(),
            blob.to_canonical_text()
        ))
    }

    /// Addresses an M1 `ExactArtifactProcedure` record (distinct from the
    /// `Artifact` projection record above; graph-schema.md §2.6's
    /// `exact_artifact` `ProcedureKind` names the exact-artifact adapter's
    /// full metadata as the procedure's own detail, not the separate
    /// rendering-facing `Artifact` entity in §2.9).
    pub fn exact_artifact(procedure_id: ProcedureId) -> Self {
        Self(format!("exact_artifact:{}", procedure_id.to_canonical_text()))
    }

    /// Addresses a `LogicalChange` entity (graph-schema.md §2.3) by its
    /// assigned `LogicalChangeId`.
    pub fn logical_change(id: LogicalChangeId) -> Self {
        Self(format!("logical_change:{}", id.to_canonical_text()))
    }

    /// Addresses a `Ref` (graph-schema.md §2.5): a `Ref` has no identity of
    /// its own, only a mutable `name` scoped to the repository that owns it
    /// (`Repository HAS_REF Ref`, §2.5), so the key includes both.
    pub fn reference(repository_name: impl AsRef<str>, ref_name: impl AsRef<str>) -> Self {
        Self(format!(
            "ref:{}:{}",
            repository_name.as_ref(),
            ref_name.as_ref()
        ))
    }

    /// Addresses the raw byte payload a `Blob` entity's `BlobHash` refers to
    /// (graph-schema.md §2.10). `Blob` itself has no Rust storage struct in
    /// `px-repo-model` beyond its hash function; this is the PluresDB-side
    /// byte container the M1 import/materialize flow actually needs so a
    /// persisted graph can be materialized back without the original
    /// filesystem tree still being present on disk.
    pub fn blob(hash: BlobHash) -> Self {
        Self(format!("blob:{}", hash.to_canonical_text()))
    }

    /// Addresses an M1-imported empty directory bookkeeping record
    /// (`px_adapter_exact::ImportedDirectory`'s doc comment: directories are
    /// not an M0 graph entity, but must still be remembered so a
    /// materialize-after-persist round trip does not silently drop them),
    /// scoped to the repository that owns the imported tree.
    pub fn empty_directory(repository_name: impl AsRef<str>, path: impl AsRef<str>) -> Self {
        Self(format!(
            "empty_directory:{}:{}",
            repository_name.as_ref(),
            path.as_ref()
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NodeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeKind {
    Procedure,
    Revision,
    ProcedureRevision,
    Artifact,
    ExactArtifact,
    LogicalChange,
    Ref,
    Blob,
    EmptyDirectory,
}

/// PluresDB-side raw byte container for a `Blob` entity (graph-schema.md
/// §2.10). Not an M0 Rust type - `px_repo_model::schema::Blob` already
/// exists and is reused verbatim as the payload; this wrapper exists only so
/// the node envelope can carry the same self-consistency check every other
/// stored node gets (the caller-supplied key must match the recomputed
/// content hash), never a second definition of what a blob's identity is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRecord {
    pub content: Vec<u8>,
}

/// M1's `ImportedDirectory` bookkeeping fact, persisted so the graph
/// substrate can materialize an empty directory back even after the
/// original in-memory `ImportedTree` is gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptyDirectoryRecord {
    pub repository_name: String,
    pub path: String,
}

/// A graph node. The procedure/revision/etc. content is never copied into a
/// new storage-specific struct; it is stored and recovered verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum GraphNode {
    Procedure(Procedure),
    Revision(RevisionContent),
    ProcedureRevision(ProcedureRevisionContent),
    Artifact(Artifact),
    ExactArtifact(ExactArtifactProcedure),
    LogicalChange(LogicalChange),
    Ref(Ref),
    Blob(BlobRecord),
    EmptyDirectory(EmptyDirectoryRecord),
}

impl GraphNode {
    pub fn kind(&self) -> GraphNodeKind {
        match self {
            Self::Procedure(_) => GraphNodeKind::Procedure,
            Self::Revision(_) => GraphNodeKind::Revision,
            Self::ProcedureRevision(_) => GraphNodeKind::ProcedureRevision,
            Self::Artifact(_) => GraphNodeKind::Artifact,
            Self::ExactArtifact(_) => GraphNodeKind::ExactArtifact,
            Self::LogicalChange(_) => GraphNodeKind::LogicalChange,
            Self::Ref(_) => GraphNodeKind::Ref,
            Self::Blob(_) => GraphNodeKind::Blob,
            Self::EmptyDirectory(_) => GraphNodeKind::EmptyDirectory,
        }
    }
}

/// Versioned node record stored as the PluresDB `NodeData` JSON payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredNode {
    pub schema_version: u8,
    pub key: NodeKey,
    pub node: GraphNode,
}

impl StoredNode {
    pub(crate) const SCHEMA_VERSION: u8 = 1;

    pub(crate) fn new(key: NodeKey, node: GraphNode) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            key,
            node,
        }
    }
}
