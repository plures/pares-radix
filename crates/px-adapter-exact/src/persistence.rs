//! M3: wires the M1 import/materialize flow into the M2 PluresDB-backed
//! `px-graph-store`, per PR #601's own follow-up note ("wire
//! ProcedureRevision/artifact/ref/logical-change persistence into
//! import/materialization flows").
//!
//! `import_tree`/`materialize_tree` (this crate's M1 exit criterion) operate
//! on the in-memory [`crate::tree::ImportedTree`]. This module is the
//! missing link: [`persist_imported_tree`] writes that in-memory tree into a
//! real [`px_graph_store::GraphStore`] using the exact relationships
//! `exact-artifact-procedure.md` §2 specifies (`Procedure` +
//! `ExactArtifactProcedure` + `Artifact` + `ProcedureRevision`, plus the
//! `Repository CONTAINS Procedure` edge from graph-schema.md §2.6), and
//! [`materialize_from_store`] reads that same graph back out into a fresh
//! `ImportedTree` so it can be handed to the existing [`crate::materialize_tree`]
//! — proving the graph substrate, not just the in-memory value, is what
//! round-trips.

use px_graph_store::{EdgeKind, EdgeRecord, GraphStore, GraphStoreError, NodeKey};
use px_repo_model::identity::ProcedureId;
use px_repo_model::schema::{
    procedure_revision_hash, Artifact, Procedure, ProcedureKind, ProcedureRevisionContent,
};

use crate::tree::{ImportedDirectory, ImportedFile, ImportedTree};

#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error(transparent)]
    GraphStore(#[from] GraphStoreError),
    #[error("canonical hashing failed while persisting a procedure revision: {0}")]
    Canonical(#[from] px_repo_model::canonical::CanonicalError),
}

/// Persist every directory and file/symlink in `tree` into `store` under
/// `repository_name`, following the exact `Procedure`/`ExactArtifactProcedure`
/// /`Artifact`/`ProcedureRevision` relationship exact-artifact-procedure.md
/// §2 defines. Idempotent: re-persisting the same `ImportedTree` writes the
/// same content-addressed records again (`CrdtStore::put` semantics), so
/// calling this twice for an unchanged tree is a no-op in effect.
pub fn persist_imported_tree(
    store: &GraphStore,
    repository_name: &str,
    tree: &ImportedTree,
) -> Result<(), PersistError> {
    for directory in &tree.directories {
        store.put_empty_directory(repository_name, &directory.path)?;
    }

    for file in &tree.files {
        persist_imported_file(store, repository_name, file)?;
    }

    Ok(())
}

fn persist_imported_file(
    store: &GraphStore,
    repository_name: &str,
    file: &ImportedFile,
) -> Result<(), PersistError> {
    let procedure_id = file.artifact.procedure_id;

    // The blob is only meaningful for non-symlinks (a symlink's payload is
    // its target string, per the M1 importer's own doc comment); persisting
    // an empty blob for a symlink would still be correct (its hash is the
    // BLAKE3 of zero bytes) but is skipped as pointless duplicate writes of
    // the same well-known empty-content hash across every symlink.
    if !file.artifact.is_symlink {
        store.put_blob(file.blob.clone())?;
    }

    store.put_exact_artifact(file.artifact.clone())?;

    store.put_artifact(Artifact {
        owning_procedure: procedure_id,
        blob: file.artifact.blob,
        mode: file.artifact.mode.clone(),
        encoding: file.artifact.encoding.clone(),
        is_symlink: file.artifact.is_symlink,
        symlink_target: file.artifact.symlink_target.clone(),
    })?;

    // exact-artifact-procedure.md §2: the ProcedureRevision's source_text is
    // "the canonical .px serialization of the ExactArtifactProcedure fields"
    // (not the raw file content, which lives in the Blob/Artifact). This
    // crate does not implement the full `.px` grammar (out of scope per this
    // crate's own doc comment), so it uses the same canonical-JSON encoding
    // `px-repo-model` already defines and hashes everything else with
    // (ADR-0040 §2) as the deterministic structured-metadata serialization;
    // this is a real, well-defined encoding, not a placeholder string.
    let source_text = px_repo_model::canonical::to_canonical_bytes(&file.artifact)
        .map(|bytes| String::from_utf8(bytes).expect("canonical JSON is always valid UTF-8"))?;

    let revision_content = ProcedureRevisionContent {
        procedure_id,
        source_text,
        capability_grants: vec![],
        residuals: vec![],
    };
    let revision_hash = procedure_revision_hash(&revision_content)?;
    store.put_procedure_revision(revision_hash, revision_content)?;

    let name = file_name(&file.artifact.original_path);
    let procedure = Procedure {
        id: procedure_id,
        kind: ProcedureKind::ExactArtifact,
        name,
        path: file.artifact.original_path.clone(),
        current_revision: revision_hash,
    };
    store.put_procedure(procedure)?;

    store.put_edge(EdgeRecord::new(
        NodeKey::repository(repository_name),
        EdgeKind::RepositoryContainsProcedure,
        NodeKey::procedure(procedure_id),
    ))?;

    Ok(())
}

fn file_name(posix_path: &str) -> String {
    posix_path
        .rsplit('/')
        .next()
        .unwrap_or(posix_path)
        .to_owned()
}

/// Reconstruct an [`ImportedTree`] entirely from what is persisted in
/// `store` under `repository_name` — no reference to the original
/// filesystem or the in-memory tree that was persisted. This is the
/// substrate-backed half of the M1 round-trip guarantee: the result is
/// handed to [`crate::materialize_tree`] the same way a freshly imported
/// `ImportedTree` would be.
pub fn materialize_from_store(
    store: &GraphStore,
    repository_name: &str,
) -> Result<ImportedTree, PersistError> {
    let mut tree = ImportedTree::default();

    for path in store.get_empty_directories(repository_name)? {
        tree.directories.push(ImportedDirectory { path });
    }

    let procedure_keys = store.get_children(
        &NodeKey::repository(repository_name),
        EdgeKind::RepositoryContainsProcedure,
    )?;
    for key in procedure_keys {
        let procedure_id = procedure_id_from_key(&key);
        let Some(artifact) = store.get_exact_artifact(procedure_id)? else {
            continue;
        };
        let blob = if artifact.is_symlink {
            px_repo_model::schema::Blob { content: Vec::new() }
        } else {
            store
                .get_blob(artifact.blob)?
                .expect("a persisted non-symlink ExactArtifactProcedure always has its blob persisted alongside it")
        };
        tree.files.push(ImportedFile { artifact, blob });
    }

    tree.canonicalize();
    Ok(tree)
}

/// Recover the `ProcedureId` a `procedure:<uuid>` `NodeKey` addresses. This
/// mirrors `NodeKey::procedure`'s own encoding rather than adding a second
/// parsing rule to `px-graph-store`'s public API, since only this crate's
/// reconstruction path needs to walk node keys back into typed ids.
fn procedure_id_from_key(key: &NodeKey) -> ProcedureId {
    let text = key
        .as_str()
        .strip_prefix("procedure:")
        .expect("get_children(RepositoryContainsProcedure) only ever returns procedure: keys");
    ProcedureId::parse_canonical_text(text)
        .expect("NodeKey::procedure always encodes a valid canonical ProcedureId")
}
