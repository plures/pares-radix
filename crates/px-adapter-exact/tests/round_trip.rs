//! M1 exit-criterion test: import a real, non-trivial directory tree,
//! materialize it back out, and diff against the original byte-for-byte.
//!
//! This directly exercises design-spec §19 conformance test 3 ("Exact
//! folder import round-trips byte-for-byte") at the folder level (the M0
//! `exact_artifact.rs` tests only cover the per-artifact metadata contract).

use std::fs;
use std::path::Path;

use px_adapter_exact::{import_tree, materialize_tree};

/// Build a real, non-trivial fixture directory tree under `root`:
/// multiple files, nested directories, an empty directory, and a binary
/// file (not valid UTF-8), matching the task's required fixture shape.
fn build_fixture(root: &Path) {
    fs::create_dir_all(root.join("src/nested")).unwrap();
    fs::create_dir_all(root.join("empty-dir")).unwrap();

    fs::write(root.join("README.md"), b"# Fixture\n\nSome text with LF endings.\n").unwrap();
    fs::write(root.join("src/main.rs"), b"fn main() {\r\n    println!(\"hi\");\r\n}\r\n").unwrap();
    fs::write(
        root.join("src/nested/mixed.txt"),
        b"line one\nline two\r\nline three\n",
    )
    .unwrap();

    // A genuine binary file: not valid UTF-8 and containing NUL bytes /
    // arbitrary byte values, e.g. a tiny fake "image" payload.
    let binary_content: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0xFF, 0xFE, 0x80, 0x81, 0x00, 0x00, 0x01];
    fs::write(root.join("src/nested/asset.bin"), &binary_content).unwrap();

    // A no-trailing-newline single-line file (line-ending edge case).
    fs::write(root.join("no-newline.txt"), b"no trailing newline here").unwrap();
}

/// Recursively collect every regular file's relative path + content under
/// `root`, sorted, for a strict byte-for-byte comparison. Directories are
/// compared separately via `collect_dirs`.
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
            let content = fs::read(entry.path()).unwrap();
            out.push((relative, content));
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
            let relative = entry
                .path()
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            out.push(relative);
        }
    }
    out.sort();
    out
}

#[test]
fn round_trip_import_then_materialize_is_byte_identical() {
    let source_dir = tempfile::tempdir().unwrap();
    build_fixture(source_dir.path());

    let tree = import_tree(source_dir.path()).expect("import must succeed on a well-formed fixture");

    // Every imported artifact must pass the M0 field-level validation
    // contract (mode/encoding/line-ending invariants).
    for file in &tree.files {
        file.artifact
            .validate()
            .expect("every imported ExactArtifactProcedure must satisfy the M0 validate() contract");
    }

    let dest_dir = tempfile::tempdir().unwrap();
    // materialize_tree requires the root to not already exist or be empty;
    // remove the auto-created tempdir and let materialize_tree recreate it.
    fs::remove_dir(dest_dir.path()).unwrap();
    materialize_tree(&tree, dest_dir.path()).expect("materialization must succeed");

    let original_files = collect_files(source_dir.path());
    let materialized_files = collect_files(dest_dir.path());
    assert_eq!(
        original_files, materialized_files,
        "materialized file paths and byte content must exactly match the original fixture"
    );

    let original_dirs = collect_dirs(source_dir.path());
    let materialized_dirs = collect_dirs(dest_dir.path());
    assert_eq!(
        original_dirs, materialized_dirs,
        "materialized directory structure (including the empty directory) must exactly match the original"
    );
}

#[test]
fn re_importing_a_materialized_tree_yields_identical_blob_hashes() {
    // exact-artifact-procedure.md §5 "stable projection law": read(write(G))
    // is equivalent to G for the exact adapter. Concretely: re-importing a
    // materialized tree must yield the same set of (path, blob_hash) pairs.
    let source_dir = tempfile::tempdir().unwrap();
    build_fixture(source_dir.path());

    let original_tree = import_tree(source_dir.path()).unwrap();

    let dest_dir = tempfile::tempdir().unwrap();
    fs::remove_dir(dest_dir.path()).unwrap();
    materialize_tree(&original_tree, dest_dir.path()).unwrap();

    let reimported_tree = import_tree(dest_dir.path()).unwrap();

    let mut original_hashes: Vec<(String, String)> = original_tree
        .files
        .iter()
        .map(|f| (f.artifact.original_path.clone(), f.artifact.blob.to_canonical_text()))
        .collect();
    let mut reimported_hashes: Vec<(String, String)> = reimported_tree
        .files
        .iter()
        .map(|f| (f.artifact.original_path.clone(), f.artifact.blob.to_canonical_text()))
        .collect();
    original_hashes.sort();
    reimported_hashes.sort();

    assert_eq!(original_hashes, reimported_hashes);
}

#[test]
fn materialize_rejects_paths_that_would_escape_the_declared_root() {
    use px_adapter_exact::{materialize_tree, ImportedFile, ImportedTree};
    use px_repo_model::exact_artifact::{ExactArtifactProcedure, LineEndings};
    use px_repo_model::identity::{BlobHash, ProcedureId};
    use px_repo_model::schema::Blob;

    let content = b"escape attempt".to_vec();
    let artifact = ExactArtifactProcedure {
        procedure_id: ProcedureId::new(),
        blob: BlobHash::of_raw_bytes(&content),
        mode: "0644".to_owned(),
        encoding: "utf-8".to_owned(),
        is_symlink: false,
        symlink_target: None,
        original_path: "../escaped.txt".to_owned(),
        line_endings: LineEndings::Lf,
    };
    let malicious_tree = ImportedTree {
        directories: Vec::new(),
        files: vec![ImportedFile { artifact, blob: Blob { content } }],
    };

    let dest_dir = tempfile::tempdir().unwrap();
    fs::remove_dir(dest_dir.path()).unwrap();
    let result = materialize_tree(&malicious_tree, dest_dir.path());
    assert!(result.is_err(), "a '..'-containing path must be rejected, not written outside the root");
}
