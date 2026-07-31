//! Repository graph schema entity types (graph-schema.md §2). This module
//! defines the field sets and canonical-JSON shape (via `serde`) for every
//! M0 graph entity. The types deliberately model only the schema's stated
//! data; querying, persistence, import, and materialization belong to later
//! milestones and are not advertised by this crate.

use serde::{Deserialize, Serialize};

use crate::identity::{
    BlobHash, ContentHash, EntityId, LogicalChangeId, ProcedureId,
    ProcedureRevisionHash, ProjectionId, RenderingProfileHash, RevisionId,
};

/// RFC-0003 §2.1 closed `Effect` enum, adopted verbatim per ADR-0040 §5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    DbRead,
    DbWrite,
    Network,
    Shell,
    FileRead,
    FileWrite,
    EnvRead,
    Clock,
    Random,
}

/// `CapabilityGrant` value type (graph-schema.md §2.17, ADR-0040 §5).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CapabilityGrant {
    pub effect: Effect,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// `ResidualRef` nested value type (graph-schema.md §2.7).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResidualRef {
    pub blob_hash: BlobHash,
    pub kind: String,
    pub attachment_point: String,
}

/// `ProcedureKind` closed enum (graph-schema.md §2.6, mirrors design-spec
/// §5's taxonomy 5.1-5.7 exactly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureKind {
    State,
    ExactArtifact,
    Change,
    Projection,
    Reconciliation,
    Validation,
    Compatibility,
}

/// `Procedure` entity (graph-schema.md §2.6). `id`/`current_revision` are
/// not part of the entity's own hashed content when the procedure appears
/// nested inside a `procedure_root` pair (ADR-0040 §4 hashes `{procedure_id,
/// revision_hash}` pairs directly, not this whole struct) - this struct is
/// the full logical record as stored/queried, useful for the conformance
/// test's fixture graph builder even though ADR-0040 §4's Merkle rule only
/// consumes a projection of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Procedure {
    pub id: ProcedureId,
    pub kind: ProcedureKind,
    pub name: String,
    pub path: String,
    pub current_revision: ProcedureRevisionHash,
}

/// `ProcedureRevision` entity (graph-schema.md §2.7). Content-derived
/// identity: `ProcedureRevisionHash = hash_canonical_json(ProcedureRevisionContent)`
/// where `ProcedureRevisionContent` is this struct's fields minus its own
/// self-referential `hash` field (ADR-0040 §4's self-exclusion rule, applied
/// here to a per-entity hash rather than a Merkle root).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureRevisionContent {
    pub procedure_id: ProcedureId,
    pub source_text: String,
    /// Sorted by `effect` variant name then `scope`, per graph-schema.md
    /// §2.7 - caller is responsible for sorting before hashing; this type
    /// does not silently re-sort a caller-provided order (array order is
    /// semantically significant per ADR-0040 §2, so a hashing helper must
    /// never mutate it - see `canonical::hash_canonical_json`, which hashes
    /// exactly what's given).
    pub capability_grants: Vec<CapabilityGrant>,
    pub residuals: Vec<ResidualRef>,
}

/// `SemanticEntity` entity (graph-schema.md §2.8). Per ADR-0040 §4's
/// `entity_root` rule, `entity_hash` excludes the entity's own `id` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticEntityContent {
    pub entity_kind: String,
    pub name: String,
    pub owning_procedure: ProcedureId,
}

/// `Blob` entity (graph-schema.md §2.10). Hashed directly over
/// `content` (ADR-0040 §3), never wrapped in canonical JSON - this struct
/// only carries the raw bytes; `BlobHash::of_raw_bytes(&content)` is how a
/// caller computes its identity, not this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    pub content: Vec<u8>,
}

impl Blob {
    pub fn hash(&self) -> BlobHash {
        BlobHash::of_raw_bytes(&self.content)
    }

    pub fn size_bytes(&self) -> u64 {
        self.content.len() as u64
    }
}

/// `Revision` entity content (graph-schema.md §2.2). Per this document's
/// resolution of the design-spec §4.4 tension: `author`/`timestamp`/`message`
/// ARE included in the hash (asserted facts about the revision), while
/// validation/attestation records are explicitly excluded (linked objects,
/// not RevisionId inputs). This struct's own `id: RevisionId` field is
/// deliberately NOT present here - `RevisionContent` is exactly the set of
/// fields that get hashed to produce `RevisionId`; the full `Revision`
/// record (with its own id attached) is a separate concern for storage,
/// out of scope for the M0 serialization conformance test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevisionContent {
    /// Order-significant (ADR-0040 §2 array rule) - not sorted.
    pub parents: Vec<RevisionId>,
    pub procedure_root: crate::identity::ContentHash,
    pub entity_root: crate::identity::ContentHash,
    /// Order-significant: the order changes were applied within this
    /// revision, not sorted (graph-schema.md §2.2).
    pub change_ids: Vec<LogicalChangeId>,
    pub rendering_profile: RenderingProfileHash,
    pub materialization_root: crate::identity::ContentHash,
    pub author: String,
    pub timestamp: String,
    pub message: String,
}

/// `Repository` entity (graph-schema.md §2.1). `created_at` is ambient
/// metadata and callers must not include this full record in a content hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    pub name: String,
    pub default_ref: String,
    pub created_at: String,
}

/// `LogicalChange` entity (graph-schema.md §2.3). `procedure_ids` is
/// unordered in the schema; use [`LogicalChange::canonicalize`] before
/// serialization when a stable representation is needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalChange {
    pub id: LogicalChangeId,
    pub kind: String,
    pub summary: String,
    pub procedure_ids: Vec<ProcedureId>,
}

impl LogicalChange {
    pub fn canonicalize(&mut self) {
        self.procedure_ids
            .sort_by_key(|procedure_id| procedure_id.to_canonical_text());
    }
}

/// Local, reconstructable `Workspace` context (graph-schema.md §2.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub root_path: String,
    pub checked_out_revision: RevisionId,
    pub rendering_profile: RenderingProfileHash,
}

/// Mutable named pointer to a revision (graph-schema.md §2.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ref {
    pub name: String,
    pub target: RevisionId,
}

/// Projection-facing output whose content identity is its referenced blob
/// (graph-schema.md §2.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub owning_procedure: ProcedureId,
    pub blob: BlobHash,
    pub mode: String,
    pub encoding: String,
    pub is_symlink: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
}

/// Named materialization configuration (graph-schema.md §2.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projection {
    pub id: ProjectionId,
    pub name: String,
    pub adapter: String,
    pub root_path: String,
}

/// Adapter implementation pin (graph-schema.md §2.12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adapter {
    pub name: String,
    pub version: String,
    pub implementation_hash: BlobHash,
}

/// Hash input for a `RenderingProfile` (graph-schema.md §2.13). Parameters
/// are kept as a JSON object because the adapter-specific schema is
/// intentionally open in M0; canonical encoding still rejects floats and
/// sorts keys according to ADR-0040 when this value is hashed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderingProfileContent {
    pub adapter_name: String,
    pub adapter_version: String,
    pub adapter_implementation_hash: BlobHash,
    pub formatter_version: String,
    pub platform_profile: String,
    pub parameters: serde_json::Map<String, serde_json::Value>,
}

/// A validation observation is deliberately not content-addressed
/// (graph-schema.md §2.14); its timestamp/evidence must not change a
/// `RevisionId`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Validation {
    pub revision: RevisionId,
    pub kind: String,
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_blob: Option<BlobHash>,
    pub run_at: String,
}

/// Detached statement about a revision (graph-schema.md §2.15).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    pub revision: RevisionId,
    pub principal: String,
    pub statement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<Vec<u8>>,
    pub attested_at: String,
}

/// A merge/reconciliation conflict (graph-schema.md §2.16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub base: RevisionId,
    pub left: RevisionId,
    pub right: RevisionId,
    pub subject_entities: Vec<EntityId>,
    pub reason: String,
    pub alternatives: Vec<String>,
}

/// Workspace-local draft that preserves a parse-recovery state
/// (graph-schema.md §2.18).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftOverlay {
    pub artifact_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_owner: Option<ProcedureId>,
    pub content_blob: BlobHash,
    pub parser_state: String,
    pub diagnostics: Vec<String>,
}

/// External proposal metadata (graph-schema.md §2.19). `fidelity_report` is
/// an open JSON object because the referenced FidelityVector schema is
/// explicitly deferred by graph-schema.md §5; pretending it has a closed
/// Rust layout here would invent protocol semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalImport {
    pub source_ref: String,
    pub proposed_changes: Vec<LogicalChangeId>,
    pub fidelity_report: serde_json::Map<String, serde_json::Value>,
}

/// Compute a content-derived `ProcedureRevisionHash` from the exact fields
/// that define it (graph-schema.md §2.7), excluding the self hash by using
/// `ProcedureRevisionContent` rather than a record that carries its hash.
pub fn procedure_revision_hash(
    content: &ProcedureRevisionContent,
) -> Result<ProcedureRevisionHash, crate::canonical::CanonicalError> {
    crate::canonical::hash_canonical_json(content).map(ProcedureRevisionHash::from_content_hash)
}

/// Compute an entity's content hash excluding its assigned `EntityId`, as
/// mandated by ADR-0040 §4's self-exclusion rule.
pub fn semantic_entity_hash(
    content: &SemanticEntityContent,
) -> Result<ContentHash, crate::canonical::CanonicalError> {
    crate::canonical::hash_canonical_json(content)
}

/// Compute the `RevisionId` from `RevisionContent`, which intentionally has
/// no self `id` field (graph-schema.md §2.2).
pub fn revision_id(
    content: &RevisionContent,
) -> Result<RevisionId, crate::canonical::CanonicalError> {
    crate::canonical::hash_canonical_json(content).map(RevisionId::from_content_hash)
}

/// Compute a `RenderingProfileHash` from the profile's deterministic inputs.
pub fn rendering_profile_hash(
    content: &RenderingProfileContent,
) -> Result<RenderingProfileHash, crate::canonical::CanonicalError> {
    crate::canonical::hash_canonical_json(content).map(RenderingProfileHash::from_content_hash)
}
