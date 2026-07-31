//! M3 end-to-end test: import a real fixture repo via `px-adapter-exact`,
//! persist it into a real PluresDB-backed `px-graph-store`, verify it is
//! queryable from the store (nodes, children, Merkle root), and prove the
//! full `import -> persist -> materialize` path is byte-identical to the
//! M1 in-memory round trip guarantee — but now going through the persisted
//! store rather than reusing the original in-memory `ImportedTree`.

use std::fs;
use std::path::Path;

use px_adapter_exact::{import_tree, materialize_from_store, materialize_tree, persist_imported_tree};
use px_graph_store::{EdgeKind, GraphStore, NodeKey};
use px_repo_model::schema::ProcedureKind;

const REPOSITORY_NAME: &str = "e2e-fixture-repo";

fn build_fixture(root: &Path) {
    fs::create_dir_all(root.join("src/nested")).unwrap();
    fs::create_dir_all(root.join("empty-dir")).unwrap();

    fs::write(root.join("README.md"), b"# Fixture\n\nSome text with LF endings.\n").unwrap();
    fs::write(
        root.join("src/main.rs"),
        b"fn main() {\r\n    println!(\"hi\");\r\n}\r\n",
    )
    .unwrap();
    fs::write(
        root.join("src/nested/mixed.txt"),
        b"line one\nline two\r\nline three\n",
    )
    .unwrap();

    let binary_content: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0xFF, 0xFE, 0x80, 0x81, 0x00, 0x00,
        0x01,
    ];
    fs::write(root.join("src/nested/asset.bin"), &binary_content).unwrap();
    fs::write(root.join("no-newline.txt"), b"no trailing newline here").unwrap();
}

fn collect_files(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root).min_depth(1) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            out.push((relative, fs::read(entry.path()).unwrap()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn collect_dirs(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root).min_depth(1) {
        let entry = entry.unwrap();
        if entry.file_type().is_dir() {
            out.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/"),
            );
        }
    }
    out.sort();
    out
}

#[test]
fn import_persist_materialize_round_trips_through_pluresdb_byte_identical() {
    let source_dir = tempfile::tempdir().unwrap();
    build_fixture(source_dir.path());

    // --- import (M1) ---
    let tree = import_tree(source_dir.path()).expect("import must succeed on a well-formed fixture");
    assert_eq!(tree.files.len(), 5, "fixture has 5 real files");
    assert_eq!(tree.directories.len(), 3, "fixture has 3 non-root directories, one empty");

    // --- persist (M3): a real, durable PluresDB/Sled-backed store ---
    let db_dir = tempfile::tempdir().unwrap();
    {
        let store = GraphStore::open(db_dir.path()).expect("open durable PluresDB store");
        persist_imported_tree(&store, REPOSITORY_NAME, &tree).expect("persist imported tree");
    }

    // Re-open to prove durability across a fresh process-local `CrdtStore`
    // instance, exactly like px-graph-store's own M2 durability test does.
    let store = GraphStore::open(db_dir.path()).expect("re-open durable PluresDB store");

    // --- verify queryability: nodes ---
    let procedure_keys = store
        .get_children(
            &NodeKey::repository(REPOSITORY_NAME),
            EdgeKind::RepositoryContainsProcedure,
        )
        .expect("query repository -> procedure children");
    assert_eq!(procedure_keys.len(), 5, "one Procedure per imported file");

    for file in &tree.files {
        let procedure = store
            .get_procedure(file.artifact.procedure_id)
            .expect("query procedure")
            .expect("every imported file's Procedure must be persisted and queryable");
        assert_eq!(procedure.kind, ProcedureKind::ExactArtifact);
        assert_eq!(procedure.path, file.artifact.original_path);

        let stored_artifact = store
            .get_exact_artifact(file.artifact.procedure_id)
            .expect("query exact artifact")
            .expect("every imported file's ExactArtifactProcedure must be persisted");
        assert_eq!(&stored_artifact, &file.artifact);

        let stored_procedure_revision = store
            .get_procedure_revision(procedure.current_revision)
            .expect("query procedure revision")
            .expect("the Procedure's current_revision must be a persisted ProcedureRevision");
        assert_eq!(stored_procedure_revision.procedure_id, file.artifact.procedure_id);

        if !file.artifact.is_symlink {
            let stored_blob = store
                .get_blob(file.artifact.blob)
                .expect("query blob")
                .expect("every non-symlink file's Blob must be persisted under its BlobHash");
            assert_eq!(stored_blob.content, file.blob.content);
        }
    }

    // --- verify queryability: current procedure Merkle root ---
    // The persisted graph's current_procedure_root must reproduce exactly
    // the same root as computing it directly from the in-memory tree's
    // procedures (ADR-0040 §4 / px_repo_model::merkle::procedure_root),
    // proving px-graph-store did not silently drop or alter any procedure
    // during persistence.
    let expected_root = px_repo_model::merkle::procedure_root(
        tree.files
            .iter()
            .map(|f| (f.artifact.procedure_id, {
                let content = px_repo_model::schema::ProcedureRevisionContent {
                    procedure_id: f.artifact.procedure_id,
                    source_text: String::from_utf8(
                        px_repo_model::canonical::to_canonical_bytes(&f.artifact).unwrap(),
                    )
                    .unwrap(),
                    capability_grants: vec![],
                    residuals: vec![],
                };
                px_repo_model::schema::procedure_revision_hash(&content).unwrap()
            }))
            .collect(),
    )
    .unwrap();
    let persisted_root = store
        .current_procedure_root()
        .expect("compute current procedure root from persisted store");
    assert_eq!(
        persisted_root, expected_root,
        "persisted graph's Merkle root must match the root computed directly from the imported tree"
    );

    // --- reconstruct and materialize purely from the persisted store ---
    let reconstructed_tree = materialize_from_store(&store, REPOSITORY_NAME)
        .expect("reconstruct ImportedTree entirely from the persisted graph");
    assert_eq!(reconstructed_tree.files.len(), tree.files.len());
    assert_eq!(reconstructed_tree.directories.len(), tree.directories.len());

    let dest_dir = tempfile::tempdir().unwrap();
    fs::remove_dir(dest_dir.path()).unwrap();
    materialize_tree(&reconstructed_tree, dest_dir.path())
        .expect("materialize the store-reconstructed tree back to a real filesystem");

    let original_files = collect_files(source_dir.path());
    let materialized_files = collect_files(dest_dir.path());
    assert_eq!(
        original_files, materialized_files,
        "import -> persist -> materialize must be byte-identical to the original fixture"
    );

    let original_dirs = collect_dirs(source_dir.path());
    let materialized_dirs = collect_dirs(dest_dir.path());
    assert_eq!(
        original_dirs, materialized_dirs,
        "the persisted empty directory must materialize back, matching the original tree exactly"
    );
}
