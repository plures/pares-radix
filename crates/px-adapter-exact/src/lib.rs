//! `px-adapter-exact`: the M1 exact-artifact adapter (design-spec §6 "exact
//! adapter", exact-artifact-procedure.md §5). Implements the recursive
//! folder importer and deterministic materializer specified there:
//!
//! - [`import_tree`] walks a real filesystem directory and produces one
//!   [`px_repo_model::exact_artifact::ExactArtifactProcedure`] per regular
//!   file/symlink, plus the raw content [`Blob`](px_repo_model::schema::Blob)
//!   for each non-symlink file, matching graph-schema.md §2.6/§2.9/§2.10.
//! - [`materialize_tree`] writes an [`ImportedTree`] back to a filesystem
//!   root, satisfying the exact-import law from exact-artifact-procedure.md
//!   §5: `write(read(S)) = S`.
//!
//! Scope (M1, per the epic task and M0's own explicit boundary notes):
//! regular files, directories (including empty ones) and symlinks with
//! **UTF-8-representable relative paths**. Non-UTF-8 filenames and the exact
//! `.px` source-text grammar are both explicitly out of scope per
//! exact-artifact-procedure.md §3's `original_path` field note ("this
//! document does NOT resolve how a UTF-8-only `.px` text format represents a
//! non-UTF-8 filename byte-for-byte... flagged here as an open item for the
//! follow-on implementation") — this crate is that follow-on implementation
//! and inherits the same explicitly-named scope boundary rather than
//! guessing an unverified encoding scheme.

pub mod importer;
pub mod materializer;
pub mod tree;

pub use importer::{import_tree, ImportError};
pub use materializer::{materialize_tree, MaterializeError};
pub use tree::{ImportedDirectory, ImportedFile, ImportedTree};
