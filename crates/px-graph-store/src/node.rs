//! Typed graph node envelopes persisted in the flat PluresDB node keyspace.
//!
//! The payload types are the `px-repo-model` types themselves. The envelope
//! adds only a type discriminator required by storage and query dispatch; it
//! does not duplicate any domain schema fields.

use px_repo_model::identity::{ProcedureId, RevisionId};
use px_repo_model::schema::{Procedure, RevisionContent};
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
}

/// A graph node. The procedure/revision content is never copied into a new
/// storage-specific struct; it is stored and recovered verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum GraphNode {
    Procedure(Procedure),
    Revision(RevisionContent),
}

impl GraphNode {
    pub fn kind(&self) -> GraphNodeKind {
        match self {
            Self::Procedure(_) => GraphNodeKind::Procedure,
            Self::Revision(_) => GraphNodeKind::Revision,
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
