//! Explicit graph edges persisted as PluresDB records.

use serde::{Deserialize, Serialize};

use crate::NodeKey;

/// Relationships needed by the M2 graph query surface. The names and
/// direction match `graph-schema.md`: a `RevisionParentOf` edge is parent →
/// child, and `RepositoryContainsProcedure` is repository → procedure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    RepositoryContainsProcedure,
    RevisionParentOf,
    RevisionIncludesProcedureRevision,
    RepositoryHasRef,
    RefPointsToRevision,
    ProcedureProducesArtifact,
    ArtifactUsesBlob,
    RevisionRecordsLogicalChange,
    RepositoryContainsEmptyDirectory,
}

impl EdgeKind {
    fn storage_name(self) -> &'static str {
        match self {
            Self::RepositoryContainsProcedure => "repository_contains_procedure",
            Self::RevisionParentOf => "revision_parent_of",
            Self::RevisionIncludesProcedureRevision => "revision_includes_procedure_revision",
            Self::RepositoryHasRef => "repository_has_ref",
            Self::RefPointsToRevision => "ref_points_to_revision",
            Self::ProcedureProducesArtifact => "procedure_produces_artifact",
            Self::ArtifactUsesBlob => "artifact_uses_blob",
            Self::RevisionRecordsLogicalChange => "revision_records_logical_change",
            Self::RepositoryContainsEmptyDirectory => "repository_contains_empty_directory",
        }
    }
}

/// A directed relationship between two typed graph keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeRecord {
    pub from: NodeKey,
    pub kind: EdgeKind,
    pub to: NodeKey,
}

impl EdgeRecord {
    pub fn new(from: NodeKey, kind: EdgeKind, to: NodeKey) -> Self {
        Self { from, kind, to }
    }

    /// A deterministic PluresDB key makes edge writes idempotent. Node keys
    /// have fixed prefixes and canonical UUID/BLAKE3 suffixes, so this tuple
    /// encoding is unambiguous for the M2 node vocabulary.
    pub(crate) fn storage_key(&self) -> String {
        format!(
            "px_graph_edge:v1:{}:{}:{}",
            self.kind.storage_name(),
            self.from.as_str(),
            self.to.as_str()
        )
    }
}
