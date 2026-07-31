//! Recursive filesystem importer (M1 exit criterion, exact-artifact-procedure.md
//! §5 "exact import law": `read(S)` half of `write(read(S)) = S`).

use std::fs;
use std::path::{Path, PathBuf};

use px_repo_model::exact_artifact::{ExactArtifactProcedure, LineEndings};
use px_repo_model::identity::ProcedureId;
use px_repo_model::schema::Blob;
use walkdir::WalkDir;

use crate::tree::{ImportedDirectory, ImportedFile, ImportedTree};

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("import root {0:?} does not exist or is not a directory")]
    RootNotADirectory(PathBuf),
    #[error("path {0:?} is not representable as a UTF-8 POSIX-style relative path (non-UTF-8 filenames are out of M1 scope, per exact-artifact-procedure.md §3)")]
    NonUtf8Path(PathBuf),
    #[error("io error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("walkdir error: {0}")]
    WalkDir(#[from] walkdir::Error),
}

/// Recursively import a real filesystem directory into an [`ImportedTree`].
///
/// Scope (M1, see crate docs): regular files, directories, and symlinks with
/// UTF-8-representable relative paths. The importer never dereferences
/// symlinks (a symlink is captured by its raw target string, never followed
/// into the pointed-at content) and never mutates the source tree.
pub fn import_tree(root: &Path) -> Result<ImportedTree, ImportError> {
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|source| ImportError::Io { path: root.to_path_buf(), source })?;
    if !root_metadata.is_dir() {
        return Err(ImportError::RootNotADirectory(root.to_path_buf()));
    }

    let mut tree = ImportedTree::default();

    for entry in WalkDir::new(root).follow_links(false).min_depth(1) {
        let entry = entry?;
        let absolute_path = entry.path();
        let relative_path = absolute_path
            .strip_prefix(root)
            .expect("walkdir yields paths rooted at the walked root");
        let posix_path = to_posix_relative_path(relative_path)
            .ok_or_else(|| ImportError::NonUtf8Path(relative_path.to_path_buf()))?;

        let file_type = entry.file_type();
        if file_type.is_dir() {
            tree.directories.push(ImportedDirectory { path: posix_path });
            continue;
        }

        if file_type.is_symlink() {
            let target = fs::read_link(absolute_path)
                .map_err(|source| ImportError::Io { path: absolute_path.to_path_buf(), source })?;
            let target_text = target
                .to_str()
                .ok_or_else(|| ImportError::NonUtf8Path(target.clone()))?
                .to_owned();
            let mode = platform_mode(absolute_path, true)?;
            let artifact = ExactArtifactProcedure {
                procedure_id: ProcedureId::new(),
                blob: px_repo_model::identity::BlobHash::of_raw_bytes(b""),
                mode,
                encoding: "binary".to_owned(),
                is_symlink: true,
                symlink_target: Some(target_text),
                original_path: posix_path,
                line_endings: LineEndings::NotApplicable,
            };
            tree.files.push(ImportedFile { artifact, blob: Blob { content: Vec::new() } });
            continue;
        }

        if file_type.is_file() {
            let content = fs::read(absolute_path)
                .map_err(|source| ImportError::Io { path: absolute_path.to_path_buf(), source })?;
            let blob = Blob { content };
            let blob_hash = blob.hash();
            let encoding = detect_encoding(&blob.content);
            let line_endings = detect_line_endings(&blob.content, &encoding);
            let mode = platform_mode(absolute_path, is_executable(absolute_path)?)?;
            let artifact = ExactArtifactProcedure {
                procedure_id: ProcedureId::new(),
                blob: blob_hash,
                mode,
                encoding,
                is_symlink: false,
                symlink_target: None,
                original_path: posix_path,
                line_endings,
            };
            tree.files.push(ImportedFile { artifact, blob });
        }
    }

    tree.canonicalize();
    Ok(tree)
}

/// Convert a native relative path into a POSIX-style (`/`-separated) UTF-8
/// string, per px-bundle-format.md §2's "all paths are POSIX-style
/// regardless of host OS" rule. Returns `None` if any component is not
/// valid UTF-8 (out of M1 scope, exact-artifact-procedure.md §3).
fn to_posix_relative_path(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_str()?.to_owned()),
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> Result<bool, ImportError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| ImportError::Io { path: path.to_path_buf(), source })?;
    Ok(metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> Result<bool, ImportError> {
    // exact-artifact-procedure.md §3: "On platforms without POSIX permission
    // bits (Windows), the importer MUST synthesize a canonical value...
    // executable-by-convention, e.g. .exe/.bat/shebang-detected files."
    let _ = path;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    Ok(matches!(ext.as_deref(), Some("exe") | Some("bat") | Some("cmd") | Some("sh") | Some("ps1")))
}

#[cfg(unix)]
fn platform_mode(path: &Path, _is_executable_hint: bool) -> Result<String, ImportError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| ImportError::Io { path: path.to_path_buf(), source })?;
    let mode_bits = metadata.permissions().mode() & 0o777;
    Ok(format!("{:04o}", mode_bits))
}

#[cfg(not(unix))]
fn platform_mode(_path: &Path, is_executable_hint: bool) -> Result<String, ImportError> {
    // exact-artifact-procedure.md §3: synthesize 0644/0755 canonically.
    Ok(if is_executable_hint { "0755".to_owned() } else { "0644".to_owned() })
}

/// Detect whether content is valid UTF-8 text or must be treated as binary.
/// exact-artifact-procedure.md §3: "if bytes are not valid UTF-8, `encoding`
/// MUST be `binary`."
fn detect_encoding(content: &[u8]) -> String {
    if std::str::from_utf8(content).is_ok() {
        "utf-8".to_owned()
    } else {
        "binary".to_owned()
    }
}

/// Detect line-ending style for text content. Binary content is always
/// `n/a` (exact-artifact-procedure.md §3, matching `ExactArtifactProcedure`'s
/// own `validate()` invariant that binary encoding pairs with `NotApplicable`).
fn detect_line_endings(content: &[u8], encoding: &str) -> LineEndings {
    if encoding == "binary" {
        return LineEndings::NotApplicable;
    }
    let mut has_crlf = false;
    let mut has_lone_lf = false;
    let mut i = 0;
    while i < content.len() {
        if content[i] == b'\n' {
            if i > 0 && content[i - 1] == b'\r' {
                has_crlf = true;
            } else {
                has_lone_lf = true;
            }
        }
        i += 1;
    }
    match (has_crlf, has_lone_lf) {
        (true, true) => LineEndings::Mixed,
        (true, false) => LineEndings::Crlf,
        (false, true) => LineEndings::Lf,
        // No newlines at all (e.g. a single-line file with no trailing
        // newline, or an empty file): there is no line-ending evidence in
        // the content, but this is still text content, so
        // `ExactArtifactProcedure::validate()` requires a non-`NotApplicable`
        // value. Default to `Lf` (POSIX convention) rather than inventing a
        // fifth variant not present in the schema.
        (false, false) => LineEndings::Lf,
    }
}
