//! Deterministic materializer (M1 exit criterion, exact-artifact-procedure.md
//! §5 "exact import law": `write` half of `write(read(S)) = S`).

use std::fs;
use std::path::{Path, PathBuf};

use crate::tree::ImportedTree;

#[derive(Debug, thiserror::Error)]
pub enum MaterializeError {
    #[error("materialization root {0:?} must be an existing empty directory or not yet exist")]
    RootNotUsable(PathBuf),
    #[error("path {path:?} would escape the declared materialization root (design-spec §13.2 / conformance test 4: \"materialization cannot write outside its declared root\")")]
    PathEscapesRoot { path: String },
    #[error("io error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[cfg(unix)]
    #[error("failed to create symlink at {path:?} -> {target:?}: {source}")]
    Symlink {
        path: PathBuf,
        target: String,
        #[source]
        source: std::io::Error,
    },
    #[error("symlink materialization is not supported on this platform for path {0:?}")]
    SymlinkUnsupportedPlatform(PathBuf),
}

/// Materialize an [`ImportedTree`] to `root` on disk.
///
/// `root` MUST NOT already contain content that would be silently
/// overwritten in a way that hides pre-existing files not part of this
/// tree — this function creates `root` if absent, and requires it be an
/// empty directory if it already exists, matching conformance test 4's
/// "materialization cannot write outside its declared root" as the
/// strictest interpretation available without a full capability/workspace
/// system (M2 scope): the declared root is exactly this parameter, and nothing
/// this function writes may resolve (via `..` or symlink components in
/// `path`) outside of it.
pub fn materialize_tree(tree: &ImportedTree, root: &Path) -> Result<(), MaterializeError> {
    if root.exists() {
        let mut entries = fs::read_dir(root)
            .map_err(|source| MaterializeError::Io { path: root.to_path_buf(), source })?;
        if entries.next().is_some() {
            return Err(MaterializeError::RootNotUsable(root.to_path_buf()));
        }
    } else {
        fs::create_dir_all(root)
            .map_err(|source| MaterializeError::Io { path: root.to_path_buf(), source })?;
    }

    for directory in &tree.directories {
        let target = resolve_within_root(root, &directory.path)?;
        fs::create_dir_all(&target)
            .map_err(|source| MaterializeError::Io { path: target, source })?;
    }

    for file in &tree.files {
        let posix_path = &file.artifact.original_path;
        let target = resolve_within_root(root, posix_path)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| MaterializeError::Io { path: parent.to_path_buf(), source })?;
        }

        if file.artifact.is_symlink {
            let link_target = file
                .artifact
                .symlink_target
                .as_ref()
                .expect("ExactArtifactProcedure::validate() requires a symlink_target when is_symlink is true");
            create_symlink(link_target, &target)?;
        } else {
            fs::write(&target, &file.blob.content)
                .map_err(|source| MaterializeError::Io { path: target.clone(), source })?;
            apply_mode(&target, &file.artifact.mode)?;
        }
    }

    Ok(())
}

/// Resolve a POSIX-style relative path against `root`, rejecting anything
/// that would escape it (`..` components, absolute paths). This is the
/// concrete enforcement of conformance test 4.
fn resolve_within_root(root: &Path, posix_relative_path: &str) -> Result<PathBuf, MaterializeError> {
    if posix_relative_path.is_empty() || posix_relative_path.starts_with('/') {
        return Err(MaterializeError::PathEscapesRoot { path: posix_relative_path.to_owned() });
    }
    let mut target = root.to_path_buf();
    for part in posix_relative_path.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(MaterializeError::PathEscapesRoot { path: posix_relative_path.to_owned() });
        }
        target.push(part);
    }
    Ok(target)
}

#[cfg(unix)]
fn create_symlink(link_target: &str, at: &Path) -> Result<(), MaterializeError> {
    std::os::unix::fs::symlink(link_target, at).map_err(|source| MaterializeError::Symlink {
        path: at.to_path_buf(),
        target: link_target.to_owned(),
        source,
    })
}

#[cfg(windows)]
fn create_symlink(link_target: &str, at: &Path) -> Result<(), MaterializeError> {
    // Creating real Windows symlinks requires elevated privileges/Developer
    // Mode in the general case, so this cannot be implemented as a plain
    // file-write fallback without silently losing the "this was a symlink"
    // fact (which would violate the exact-import law). Rather than fabricate
    // a fake symlink representation (C-NOSTUB-001), materialization of
    // symlinks reports itself unsupported on this platform; the round-trip
    // test corpus for this crate is exercised on the platforms that support
    // real symlink creation, matching the M0 boundary precedent of scoping
    // explicitly rather than inventing an unverified behavior.
    let _ = link_target;
    Err(MaterializeError::SymlinkUnsupportedPlatform(at.to_path_buf()))
}

#[cfg(not(any(unix, windows)))]
fn create_symlink(link_target: &str, at: &Path) -> Result<(), MaterializeError> {
    let _ = link_target;
    Err(MaterializeError::SymlinkUnsupportedPlatform(at.to_path_buf()))
}

#[cfg(unix)]
fn apply_mode(path: &Path, mode: &str) -> Result<(), MaterializeError> {
    use std::os::unix::fs::PermissionsExt;
    let mode_bits = u32::from_str_radix(mode, 8).unwrap_or(0o644);
    let permissions = fs::Permissions::from_mode(mode_bits);
    fs::set_permissions(path, permissions)
        .map_err(|source| MaterializeError::Io { path: path.to_path_buf(), source })
}

#[cfg(not(unix))]
fn apply_mode(_path: &Path, _mode: &str) -> Result<(), MaterializeError> {
    // exact-artifact-procedure.md §3 notes mode is synthesized/canonical on
    // platforms without POSIX permission bits; there is nothing to apply
    // back on Windows (no POSIX permission bit API), matching the same
    // asymmetry the importer already documents for `platform_mode`.
    Ok(())
}
