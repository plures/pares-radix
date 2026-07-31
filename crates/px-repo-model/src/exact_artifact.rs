//! Exact-artifact procedure data and invariants.
//!
//! This is the universal lossless fallback described by
//! `exact-artifact-procedure.md`: raw file bytes remain in a [`BlobHash`],
//! while this record captures the metadata needed to materialize them without
//! transformation. Import/export I/O belongs to the M1 exact adapter; this
//! crate owns the durable, validated graph representation.

use serde::{Deserialize, Serialize};

use crate::identity::{BlobHash, ProcedureId};

/// The canonical line-ending labels fixed by `exact-artifact-procedure.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineEndings {
    Lf,
    Crlf,
    Mixed,
    #[serde(rename = "n/a")]
    NotApplicable,
}

/// Metadata for a `ProcedureKind::ExactArtifact` procedure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactArtifactProcedure {
    pub procedure_id: ProcedureId,
    pub blob: BlobHash,
    /// Four-digit POSIX permission representation, such as `0644` or `0755`.
    pub mode: String,
    /// Declared encoding, with `binary` for non-text bytes.
    pub encoding: String,
    pub is_symlink: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
    /// POSIX-style original path. The M0 spec deliberately leaves exact
    /// non-UTF-8 filename encoding open, so this String is valid only for
    /// paths expressible in canonical UTF-8 `.px` text.
    pub original_path: String,
    pub line_endings: LineEndings,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExactArtifactError {
    #[error("mode {0:?} must be exactly four ASCII octal digits")]
    InvalidMode(String),
    #[error("original path must be non-empty, relative, and POSIX-style")]
    InvalidOriginalPath,
    #[error("a symlink requires a non-empty symlink target")]
    MissingSymlinkTarget,
    #[error("a non-symlink must not carry a symlink target")]
    UnexpectedSymlinkTarget,
    #[error("binary content must use line_endings = n/a")]
    BinaryLineEndingsMustBeNotApplicable,
    #[error("text content must use line_endings other than n/a")]
    TextLineEndingsMustBeDeclared,
}

impl ExactArtifactProcedure {
    /// Validate the field-level contract that M0 can establish without
    /// filesystem I/O. This does not inspect the blob: actual byte recovery
    /// is the M1 exact-adapter responsibility.
    pub fn validate(&self) -> Result<(), ExactArtifactError> {
        if self.mode.len() != 4 || !self.mode.bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
            return Err(ExactArtifactError::InvalidMode(self.mode.clone()));
        }
        if self.original_path.is_empty()
            || self.original_path.starts_with('/')
            || self.original_path.contains('\\')
        {
            return Err(ExactArtifactError::InvalidOriginalPath);
        }
        match (self.is_symlink, self.symlink_target.as_deref()) {
            (true, Some(target)) if !target.is_empty() => {}
            (true, _) => return Err(ExactArtifactError::MissingSymlinkTarget),
            (false, Some(_)) => return Err(ExactArtifactError::UnexpectedSymlinkTarget),
            (false, None) => {}
        }
        if self.encoding == "binary" && self.line_endings != LineEndings::NotApplicable {
            return Err(ExactArtifactError::BinaryLineEndingsMustBeNotApplicable);
        }
        if self.encoding != "binary" && self.line_endings == LineEndings::NotApplicable {
            return Err(ExactArtifactError::TextLineEndingsMustBeDeclared);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> ExactArtifactProcedure {
        ExactArtifactProcedure {
            procedure_id: ProcedureId::new(),
            blob: BlobHash::of_raw_bytes(b"hello\n"),
            mode: "0644".to_owned(),
            encoding: "utf-8".to_owned(),
            is_symlink: false,
            symlink_target: None,
            original_path: "src/main.rs".to_owned(),
            line_endings: LineEndings::Lf,
        }
    }

    #[test]
    fn accepts_a_real_text_artifact_record() {
        assert_eq!(valid().validate(), Ok(()));
    }

    #[test]
    fn rejects_invalid_metadata_combinations() {
        let mut artifact = valid();
        artifact.mode = "644".to_owned();
        assert!(matches!(artifact.validate(), Err(ExactArtifactError::InvalidMode(_))));

        let mut artifact = valid();
        artifact.is_symlink = true;
        assert_eq!(artifact.validate(), Err(ExactArtifactError::MissingSymlinkTarget));

        let mut artifact = valid();
        artifact.encoding = "binary".to_owned();
        assert_eq!(
            artifact.validate(),
            Err(ExactArtifactError::BinaryLineEndingsMustBeNotApplicable)
        );
    }
}
