//! In-memory representation of an imported filesystem tree (M1 scope).
//!
//! This is the graph-shaped intermediate produced by [`crate::import_tree`]
//! and consumed by [`crate::materialize_tree`]. It is deliberately NOT a
//! PluresDB-persisted graph (that is M2 scope, design-spec §17) — this crate
//! owns only the exact-adapter's `read`/`write` functions (design-spec §6),
//! operating on an in-memory value that already has the shape of the M0
//! schema types (`ExactArtifactProcedure`, `Blob`).

use px_repo_model::exact_artifact::ExactArtifactProcedure;
use px_repo_model::schema::Blob;

/// One imported directory (graph-schema.md §2.6 does not model directories
/// as their own entity kind — a directory's existence is recorded here only
/// so an exact re-materialization can recreate EMPTY directories, which
/// would otherwise be silently lost since they own no artifact/blob of
/// their own).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedDirectory {
    /// POSIX-style path relative to the import root (`""` for the root
    /// itself is never recorded; only non-root directories appear here).
    pub path: String,
}

/// One imported regular file or symlink.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedFile {
    /// The `ExactArtifactProcedure` metadata record for this artifact
    /// (exact-artifact-procedure.md §3). `artifact.original_path` is the
    /// same POSIX-style relative path used to key this entry.
    pub artifact: ExactArtifactProcedure,
    /// Raw file content. For a symlink, this is empty — a symlink's
    /// identity-relevant payload is `artifact.symlink_target`, not file
    /// content (exact-artifact-procedure.md §3: "an exact import preserves
    /// the symlink itself"; the target it points to is not itself walked
    /// or dereferenced by this importer).
    pub blob: Blob,
}

/// A full imported filesystem tree: every directory and every file/symlink,
/// keyed by their POSIX-style path relative to the import root.
///
/// Entries are stored sorted by path (byte-wise ascending, matching
/// ADR-0040 §4's `materialization_root` sort rule) so that two imports of
/// the same directory tree always produce identical in-memory ordering,
/// independent of the host filesystem's directory-listing order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportedTree {
    pub directories: Vec<ImportedDirectory>,
    pub files: Vec<ImportedFile>,
}

impl ImportedTree {
    /// Sort `directories` and `files` by their path field, byte-wise
    /// ascending on the UTF-8 path bytes (ADR-0040 §4 convention).
    pub fn canonicalize(&mut self) {
        self.directories.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        self.files
            .sort_by(|a, b| a.artifact.original_path.as_bytes().cmp(b.artifact.original_path.as_bytes()));
    }
}
