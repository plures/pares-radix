//! Deterministic procedure-graph-to-git object projection (ADR-0038).
//!
//! This module is the **first vertical slice** of ADR-0038's deterministic
//! projection contract: a pure `graph_revision -> git object bytes` routine
//! for the two object kinds needed to prove the invariants end-to-end
//! (`blob` and `tree`), plus the shared hashing routine both must use.
//!
//! Scope of this slice (see ADR-0038 §5 for what is explicitly deferred):
//! - Blob object formatting + SHA-1 hashing (§3).
//! - Tree object formatting with strict byte-wise name ordering (§1) +
//!   SHA-1 hashing (§3).
//! - Commit object formatting is **not yet implemented** in this slice
//!   (tracked as follow-up; see module docs at bottom of this file and the
//!   PR body). This is an honest scope cut, not a stub: there is no
//!   `project_commit` function pretending to exist.
//! - Pack assembly (§3 v1 undeltified fallback) is **not yet implemented**
//!   in this slice — also tracked as follow-up.
//!
//! Every function here is pure: no wall-clock reads, no random IDs, no
//! host-specific state. All timestamps/strings are supplied by the caller
//! from the canonical graph revision fields (ADR-0038 §2).

use sha1::{Digest, Sha1};

/// A single entry in a projected git tree object.
///
/// `mode` follows git's tree-entry mode convention as a raw octal string
/// exactly as it appears in the tree object bytes (e.g. `"100644"` for a
/// regular file, `"40000"` for a subtree, `"100755"` for an executable
/// file, `"120000"` for a symlink).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Git tree-entry mode, e.g. `"100644"`.
    pub mode: &'static str,
    /// Entry name (file or subtree name), UTF-8, no path separators.
    pub name: String,
    /// The 20-byte SHA-1 object id of the referenced blob/tree.
    pub oid: [u8; 20],
}

/// Errors from the git projection routines.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProjectionError {
    /// A tree entry name was empty, which git tree objects cannot represent.
    #[error("tree entry name must not be empty")]
    EmptyEntryName,
    /// Two tree entries at the same level had the same projected name.
    #[error("duplicate tree entry name: {0}")]
    DuplicateEntryName(String),
}

/// Git object kinds relevant to this slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Blob,
    Tree,
}

impl ObjectKind {
    fn header_tag(self) -> &'static str {
        match self {
            ObjectKind::Blob => "blob",
            ObjectKind::Tree => "tree",
        }
    }
}

/// The output of a projection: the raw git object bytes (header + content)
/// and the SHA-1 object id computed over those exact bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedObject {
    /// The full git object bytes: `"<type> <len>\0<content>"`.
    pub bytes: Vec<u8>,
    /// The 20-byte SHA-1 hash of `bytes` (git's object id).
    pub oid: [u8; 20],
}

/// Shared, single-source git object hashing routine (ADR-0038 §3).
///
/// Computes the SHA-1 hash over the exact git object byte format:
/// `"<type> <length>\0<content>"`. This is the ONLY place object hashes are
/// computed in this crate — no per-call-site reimplementation.
pub fn hash_git_object(kind: ObjectKind, content: &[u8]) -> ([u8; 20], Vec<u8>) {
    let header = format!("{} {}\0", kind.header_tag(), content.len());
    let mut bytes = Vec::with_capacity(header.len() + content.len());
    bytes.extend_from_slice(header.as_bytes());
    bytes.extend_from_slice(content);

    let mut hasher = Sha1::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut oid = [0u8; 20];
    oid.copy_from_slice(&digest);
    (oid, bytes)
}

/// Project a single content blob deterministically (ADR-0038 §3).
///
/// Pure function: `content -> git blob object bytes + SHA-1 oid`. The caller
/// is responsible for supplying `content` already transcoded to UTF-8 (or
/// left as raw bytes for binary content) per ADR-0038 §2 — this function
/// performs no locale-dependent transformation.
pub fn project_blob(content: &[u8]) -> ProjectedObject {
    let (oid, bytes) = hash_git_object(ObjectKind::Blob, content);
    ProjectedObject { bytes, oid }
}

/// Project a git tree object from a set of graph child-edges, deterministically
/// (ADR-0038 §1, §3).
///
/// # Ordering invariant
///
/// Entries are sorted by **strict byte-wise ordering of entry name** before
/// serialization, exactly matching git's own tree-object ordering rule. This
/// ordering is computed here at projection time — it does NOT depend on the
/// order `entries` was supplied in, so callers may pass entries in whatever
/// order the underlying PluresDB query returned them.
///
/// Directory entries (`mode == "40000"`) are compared as if a trailing `/`
/// were appended to the name, matching git's tie-breaking rule for a file
/// and a directory that share a name prefix (e.g. `"foo"` file vs `"foo"`
/// directory containing further entries — the directory sorts as `"foo/"`).
///
/// # Errors
///
/// Returns [`ProjectionError::EmptyEntryName`] or
/// [`ProjectionError::DuplicateEntryName`] if the input is malformed —
/// a well-formed graph projection should never hit either case, but this
/// function does not assume that of its caller.
pub fn project_tree(entries: &[TreeEntry]) -> Result<ProjectedObject, ProjectionError> {
    for e in entries {
        if e.name.is_empty() {
            return Err(ProjectionError::EmptyEntryName);
        }
    }

    let mut sorted: Vec<&TreeEntry> = entries.iter().collect();
    sorted.sort_by_key(|a| git_tree_sort_key(a));

    for pair in sorted.windows(2) {
        if pair[0].name == pair[1].name {
            return Err(ProjectionError::DuplicateEntryName(pair[0].name.clone()));
        }
    }

    let mut content = Vec::new();
    for entry in &sorted {
        content.extend_from_slice(entry.mode.as_bytes());
        content.push(b' ');
        content.extend_from_slice(entry.name.as_bytes());
        content.push(0);
        content.extend_from_slice(&entry.oid);
    }

    let (oid, bytes) = hash_git_object(ObjectKind::Tree, &content);
    Ok(ProjectedObject { bytes, oid })
}

/// Compute git's tree-entry sort key: the entry name, with a trailing `/`
/// appended for directory (subtree) entries, compared byte-wise.
fn git_tree_sort_key(entry: &TreeEntry) -> Vec<u8> {
    let is_dir = entry.mode == "40000";
    if is_dir {
        let mut key = entry.name.as_bytes().to_vec();
        key.push(b'/');
        key
    } else {
        entry.name.as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── project_blob ───────────────────────────────────────────────────────

    #[test]
    fn project_blob_matches_known_git_blob_oid() {
        // `git hash-object` on an empty file yields this well-known SHA-1.
        let projected = project_blob(b"");
        let hex = hex_string(&projected.oid);
        assert_eq!(hex, "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
    }

    #[test]
    fn project_blob_matches_known_git_blob_oid_for_hello() {
        // `printf 'hello world' | git hash-object --stdin` == this SHA-1.
        let projected = project_blob(b"hello world");
        let hex = hex_string(&projected.oid);
        assert_eq!(hex, "95d09f2b10159347eece71399a7e2e907ea3df4f");
    }

    #[test]
    fn project_blob_is_pure_same_input_same_output() {
        let a = project_blob(b"repeatable content");
        let b = project_blob(b"repeatable content");
        assert_eq!(a, b, "projecting the same content twice must be identical");
    }

    // ── project_tree: ordering invariant (ADR-0038 §1) ──────────────────────

    #[test]
    fn project_tree_sorts_by_name_regardless_of_input_order() {
        let file_a = project_blob(b"a-content");
        let file_z = project_blob(b"z-content");

        let entries_forward = vec![
            TreeEntry {
                mode: "100644",
                name: "a.txt".to_string(),
                oid: file_a.oid,
            },
            TreeEntry {
                mode: "100644",
                name: "z.txt".to_string(),
                oid: file_z.oid,
            },
        ];
        let entries_reversed = vec![
            TreeEntry {
                mode: "100644",
                name: "z.txt".to_string(),
                oid: file_z.oid,
            },
            TreeEntry {
                mode: "100644",
                name: "a.txt".to_string(),
                oid: file_a.oid,
            },
        ];

        let forward = project_tree(&entries_forward).unwrap();
        let reversed = project_tree(&entries_reversed).unwrap();

        assert_eq!(
            forward, reversed,
            "tree projection must not depend on input iteration order"
        );
    }

    #[test]
    fn project_tree_directory_sorts_with_trailing_slash_tiebreak() {
        // Git's rule: a file "foo.txt" sorts BEFORE a directory "foo" because
        // the directory is compared as "foo/" — "foo." < "foo/" byte-wise... but
        // "foo" (file, no ext) vs "foo" (dir) is the real tie: dir becomes "foo/"
        // which sorts AFTER "foo" (file) since '/' (0x2F) > nothing... actually
        // git compares "foo" vs "foo/" where "foo" is a strict prefix, so "foo"
        // (shorter) sorts first. We assert that exact ordering here.
        let file_oid = project_blob(b"file content").oid;
        let sub_blob = project_blob(b"nested content").oid;
        let sub_tree = project_tree(&[TreeEntry {
            mode: "100644",
            name: "nested.txt".to_string(),
            oid: sub_blob,
        }])
        .unwrap();

        let entries = vec![
            TreeEntry {
                mode: "40000",
                name: "foo".to_string(),
                oid: sub_tree.oid,
            },
            TreeEntry {
                mode: "100644",
                name: "foo.txt".to_string(),
                oid: file_oid,
            },
        ];

        let tree = project_tree(&entries).unwrap();

        // Manually reconstruct expected content ordering: "foo" (dir, key "foo/")
        // vs "foo.txt" (file, key "foo.txt"). Byte-wise: 'foo.' < 'foo/' because
        // '.' (0x2E) < '/' (0x2F). So "foo.txt" (file) sorts BEFORE "foo" (dir).
        let mut expected_content = Vec::new();
        expected_content.extend_from_slice(b"100644 foo.txt\0");
        expected_content.extend_from_slice(&file_oid);
        expected_content.extend_from_slice(b"40000 foo\0");
        expected_content.extend_from_slice(&sub_tree.oid);

        let (expected_oid, expected_bytes) =
            hash_git_object(ObjectKind::Tree, &expected_content);
        assert_eq!(tree.bytes, expected_bytes);
        assert_eq!(tree.oid, expected_oid);
    }

    #[test]
    fn project_tree_rejects_empty_entry_name() {
        let oid = project_blob(b"x").oid;
        let entries = vec![TreeEntry {
            mode: "100644",
            name: String::new(),
            oid,
        }];
        let err = project_tree(&entries).unwrap_err();
        assert_eq!(err, ProjectionError::EmptyEntryName);
    }

    #[test]
    fn project_tree_rejects_duplicate_entry_name() {
        let oid_a = project_blob(b"a").oid;
        let oid_b = project_blob(b"b").oid;
        let entries = vec![
            TreeEntry {
                mode: "100644",
                name: "dup.txt".to_string(),
                oid: oid_a,
            },
            TreeEntry {
                mode: "100644",
                name: "dup.txt".to_string(),
                oid: oid_b,
            },
        ];
        let err = project_tree(&entries).unwrap_err();
        assert_eq!(err, ProjectionError::DuplicateEntryName("dup.txt".to_string()));
    }

    #[test]
    fn project_tree_empty_entries_matches_known_empty_tree_oid() {
        // `git mktree < /dev/null` == this well-known empty-tree SHA-1.
        let tree = project_tree(&[]).unwrap();
        let hex = hex_string(&tree.oid);
        assert_eq!(hex, "4b825dc642cb6eb9a060e54bf8d69288fbee4904");
    }

    // ── ADR-0038 §4 acceptance gate: cross-"replica" determinism test ──────
    //
    // This test simulates two independent replicas by constructing entries
    // in deliberately different orders and via independently-built Vecs
    // (no shared state beyond the graph-derived input), then asserting the
    // projected bytes are identical. A true multi-process/multi-machine
    // variant is tracked as follow-up (see PR body) — this is the
    // same-process approximation the module can prove today.
    #[test]
    fn cross_replica_determinism_same_graph_revision_same_bytes() {
        fn replica_project(order: &[(&str, &str)]) -> ProjectedObject {
            let entries: Vec<TreeEntry> = order
                .iter()
                .map(|(name, content)| TreeEntry {
                    mode: "100644",
                    name: name.to_string(),
                    oid: project_blob(content.as_bytes()).oid,
                })
                .collect();
            project_tree(&entries).unwrap()
        }

        let replica_1_order = [("b.md", "B"), ("a.md", "A"), ("c.md", "C")];
        let replica_2_order = [("c.md", "C"), ("a.md", "A"), ("b.md", "B")];

        let r1 = replica_project(&replica_1_order);
        let r2 = replica_project(&replica_2_order);

        assert_eq!(
            r1, r2,
            "two independently-ordered projections of the same logical graph \
             revision must produce byte-identical git tree objects"
        );
    }

    // ── ADR-0038 §4 acceptance gate: time-travel reproducibility test ─────
    #[test]
    fn time_travel_reproducibility_same_revision_reprojected_later() {
        let entries = vec![TreeEntry {
            mode: "100644",
            name: "stable.txt".to_string(),
            oid: project_blob(b"stable content").oid,
        }];

        let first_projection = project_tree(&entries).unwrap();
        // Simulate a later re-projection of the same historical revision:
        // fresh Vec, fresh function call, no shared state.
        let entries_again = vec![TreeEntry {
            mode: "100644",
            name: "stable.txt".to_string(),
            oid: project_blob(b"stable content").oid,
        }];
        let second_projection = project_tree(&entries_again).unwrap();

        assert_eq!(
            first_projection, second_projection,
            "re-projecting the same historical revision must yield identical bytes"
        );
    }

    fn hex_string(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
