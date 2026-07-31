//! Merkle composition rules for graph-level roots (`procedure_root`,
//! `entity_root`, `materialization_root`) per ADR-0040 §4: a flat, sorted
//! list of `{id, hash}` pairs, hashed as one canonical JSON array.

use serde::Serialize;

use crate::canonical::hash_canonical_json;
use crate::identity::{BlobHash, ContentHash, EntityId, ProcedureId, ProcedureRevisionHash};

#[derive(Debug, Clone, Serialize)]
struct ProcedureRootEntry {
    procedure_id: ProcedureId,
    revision_hash: ProcedureRevisionHash,
}

/// Compute `procedure_root` (ADR-0040 §4): BLAKE3 of the canonical-JSON
/// array of `{procedure_id, revision_hash}` pairs, sorted by
/// `procedure_id`'s canonical text form, byte-wise ascending.
pub fn procedure_root(
    mut entries: Vec<(ProcedureId, ProcedureRevisionHash)>,
) -> Result<ContentHash, crate::canonical::CanonicalError> {
    entries.sort_by_key(|(id, _)| id.to_canonical_text());
    let payload: Vec<ProcedureRootEntry> = entries
        .into_iter()
        .map(|(procedure_id, revision_hash)| ProcedureRootEntry {
            procedure_id,
            revision_hash,
        })
        .collect();
    hash_canonical_json(&payload)
}

#[derive(Debug, Clone, Serialize)]
struct EntityRootEntry {
    entity_id: EntityId,
    entity_hash: ContentHash,
}

/// Compute `entity_root` (ADR-0040 §4): BLAKE3 of the canonical-JSON array
/// of `{entity_id, entity_hash}` pairs, sorted by `entity_id`.
/// `entity_hash` is the caller-supplied per-entity content hash (already
/// computed excluding the entity's own `entity_id`, per ADR-0040 §4).
pub fn entity_root(
    mut entries: Vec<(EntityId, ContentHash)>,
) -> Result<ContentHash, crate::canonical::CanonicalError> {
    entries.sort_by_key(|(id, _)| id.to_canonical_text());
    let payload: Vec<EntityRootEntry> = entries
        .into_iter()
        .map(|(entity_id, entity_hash)| EntityRootEntry {
            entity_id,
            entity_hash,
        })
        .collect();
    hash_canonical_json(&payload)
}

#[derive(Debug, Clone, Serialize)]
struct MaterializationRootEntry {
    path: String,
    blob_hash: BlobHash,
}

/// Compute `materialization_root` (ADR-0040 §4): BLAKE3 of the
/// canonical-JSON array of `{path, blob_hash}` pairs, sorted by `path`
/// byte-wise ascending on the UTF-8 POSIX-style path string.
pub fn materialization_root(
    mut entries: Vec<(String, BlobHash)>,
) -> Result<ContentHash, crate::canonical::CanonicalError> {
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let payload: Vec<MaterializationRootEntry> = entries
        .into_iter()
        .map(|(path, blob_hash)| MaterializationRootEntry { path, blob_hash })
        .collect();
    hash_canonical_json(&payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedure_root_is_order_independent_at_the_input_level() {
        let a = ProcedureId::new();
        let b = ProcedureId::new();
        let hash_a = ProcedureRevisionHash::of_raw_bytes(b"a");
        let hash_b = ProcedureRevisionHash::of_raw_bytes(b"b");

        let root_1 = procedure_root(vec![(a, hash_a), (b, hash_b)]).unwrap();
        let root_2 = procedure_root(vec![(b, hash_b), (a, hash_a)]).unwrap();
        assert_eq!(
            root_1, root_2,
            "sorting by procedure_id must make input order irrelevant"
        );
    }

    #[test]
    fn materialization_root_sorts_paths_byte_wise() {
        let blob = BlobHash::of_raw_bytes(b"x");
        let root_1 = materialization_root(vec![
            ("z/file.txt".to_string(), blob),
            ("a/file.txt".to_string(), blob),
        ])
        .unwrap();
        let root_2 = materialization_root(vec![
            ("a/file.txt".to_string(), blob),
            ("z/file.txt".to_string(), blob),
        ])
        .unwrap();
        assert_eq!(root_1, root_2);
    }

    #[test]
    fn different_content_produces_different_roots() {
        let a = ProcedureId::new();
        let hash_a = ProcedureRevisionHash::of_raw_bytes(b"a");
        let hash_a2 = ProcedureRevisionHash::of_raw_bytes(b"a-changed");
        let root_1 = procedure_root(vec![(a, hash_a)]).unwrap();
        let root_2 = procedure_root(vec![(a, hash_a2)]).unwrap();
        assert_ne!(root_1, root_2);
    }
}
