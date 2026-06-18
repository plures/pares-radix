//! File node — the core data structure for indexed files.
//!
//! Every file in the index is a `FileNode` with two layers of data:
//! - Pass 1 fields: populated immediately on discovery (fast, no LLM)
//! - Pass 2 fields: populated asynchronously by LLM enrichment

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Identifies which system/node a file lives on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeIdentity {
    /// Unique node name in the cluster (e.g., "praxisbot", "surface", "devbox")
    pub node_id: String,
    /// Hostname for display
    pub hostname: String,
    /// OS family
    pub os: String,
    /// Architecture
    pub arch: String,
}

impl NodeIdentity {
    pub fn local() -> Self {
        Self {
            node_id: hostname(),
            hostname: hostname(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| {
            std::fs::read_to_string("/etc/hostname")
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        })
}

/// Content type classification for extraction strategy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContentClass {
    /// Plain text, markdown, config files
    Text,
    /// Source code (Rust, TypeScript, Python, Nix, etc.)
    Code { language: String },
    /// PDF documents
    Pdf,
    /// Images (PNG, JPEG, WebP, SVG)
    Image,
    /// Office documents (DOCX, XLSX, PPTX)
    Office,
    /// Audio files
    Audio,
    /// Video files
    Video,
    /// Executable binaries (ELF, PE, Mach-O)
    Binary,
    /// Archives (tar, zip, etc.)
    Archive,
    /// Unknown / unprocessable
    Unknown,
}

/// Security assessment for a file (primarily for binaries).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityProfile {
    /// Overall risk score (0.0 = safe, 1.0 = high risk)
    pub risk_score: f32,
    /// Shannon entropy of the file (high = packed/encrypted)
    pub entropy: f32,
    /// Imported libraries / syscalls (for binaries)
    pub imports: Vec<String>,
    /// Capabilities detected (network, filesystem, crypto, etc.)
    pub capabilities: Vec<String>,
    /// Whether the file is signed
    pub signed: bool,
    /// Anomaly flags
    pub anomalies: Vec<String>,
}

/// A file in the omniscient index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    // ── Identity (globally unique across cluster) ──
    /// Which system this file lives on
    pub node: NodeIdentity,
    /// Absolute path on that system
    pub path: String,
    /// SHA-256 of file content (for dedup across nodes)
    pub content_hash: String,

    // ── Pass 1: Immediate metadata ──
    pub mime: String,
    pub content_class: ContentClass,
    pub size: u64,
    pub modified: DateTime<Utc>,
    pub permissions: u32,
    /// Extracted text content (truncated for large files)
    pub extracted_text: Option<String>,
    /// Raw embedding from bge-small or CLIP
    pub raw_vector: Option<Vec<f32>>,
    /// When Pass 1 completed
    pub indexed_at: DateTime<Utc>,

    // ── Pass 2: LLM-enriched (nullable until processed) ──
    /// Human-readable summary from LLM
    pub summary: Option<String>,
    /// Extracted entities (people, projects, APIs, etc.)
    pub entities: Option<Vec<String>>,
    /// Purpose classification ("deployment", "security", "test", etc.)
    pub purpose: Option<String>,
    /// Security assessment (primarily for binaries)
    pub security: Option<SecurityProfile>,
    /// Graph relationship IDs (links to other FileNodes)
    pub relationships: Vec<String>,
    /// Re-embedded vector from enriched summary
    pub enriched_vector: Option<Vec<f32>>,
    /// When Pass 2 completed
    pub enriched_at: Option<DateTime<Utc>>,
}

/// Builder for constructing FileNodes from Pass 1 extraction.
pub struct FileNodeBuilder {
    node: NodeIdentity,
    path: String,
}

impl FileNodeBuilder {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            node: NodeIdentity::local(),
            path: path.into(),
        }
    }

    pub fn with_node(mut self, node: NodeIdentity) -> Self {
        self.node = node;
        self
    }

    /// Build a FileNode from filesystem metadata (Pass 1).
    /// Reads the file, computes hash, detects MIME type.
    pub fn build_from_fs(&self) -> std::io::Result<FileNode> {
        use sha2::{Digest, Sha256};

        let metadata = std::fs::metadata(&self.path)?;
        let content = std::fs::read(&self.path)?;
        let hash = format!("{:x}", Sha256::digest(&content));
        let mime = mime_guess::from_path(&self.path)
            .first_or_octet_stream()
            .to_string();
        let content_class = classify_mime(&mime, &self.path);
        // POSIX mode where available; on non-Unix (Windows) derive a portable
        // approximation from the read-only flag so `permissions: u32` stays meaningful
        // cross-platform. (Pre-existing code was Unix-only and broke the Windows build.)
        let permissions: u32 = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode()
            }
            #[cfg(not(unix))]
            {
                if metadata.permissions().readonly() {
                    0o444
                } else {
                    0o644
                }
            }
        };
        let modified = metadata
            .modified()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now());

        Ok(FileNode {
            node: self.node.clone(),
            path: self.path.clone(),
            content_hash: hash,
            mime,
            content_class,
            size: metadata.len(),
            modified,
            permissions,
            extracted_text: None,
            raw_vector: None,
            indexed_at: Utc::now(),
            summary: None,
            entities: None,
            purpose: None,
            security: None,
            relationships: Vec::new(),
            enriched_vector: None,
            enriched_at: None,
        })
    }
}

fn classify_mime(mime: &str, path: &str) -> ContentClass {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match mime.split('/').next().unwrap_or("") {
        "text" => match ext {
            "rs" => ContentClass::Code {
                language: "rust".into(),
            },
            "ts" | "tsx" => ContentClass::Code {
                language: "typescript".into(),
            },
            "js" | "jsx" => ContentClass::Code {
                language: "javascript".into(),
            },
            "py" => ContentClass::Code {
                language: "python".into(),
            },
            "nix" => ContentClass::Code {
                language: "nix".into(),
            },
            "go" => ContentClass::Code {
                language: "go".into(),
            },
            "c" | "h" => ContentClass::Code {
                language: "c".into(),
            },
            "cpp" | "cc" | "cxx" | "hpp" => ContentClass::Code {
                language: "cpp".into(),
            },
            "sh" | "bash" | "zsh" => ContentClass::Code {
                language: "shell".into(),
            },
            "toml" | "yaml" | "yml" | "json" | "xml" => ContentClass::Text,
            "md" | "txt" | "log" | "csv" | "ini" | "cfg" => ContentClass::Text,
            _ => ContentClass::Text,
        },
        "image" => ContentClass::Image,
        "audio" => ContentClass::Audio,
        "video" => ContentClass::Video,
        "application" => {
            match ext {
                "pdf" => ContentClass::Pdf,
                "docx" | "doc" => ContentClass::Office,
                "xlsx" | "xls" => ContentClass::Office,
                "pptx" | "ppt" => ContentClass::Office,
                "zip" | "tar" | "gz" | "xz" | "bz2" | "7z" | "rar" => ContentClass::Archive,
                "exe" | "dll" | "so" | "dylib" => ContentClass::Binary,
                "wasm" => ContentClass::Binary,
                _ => {
                    // Check for ELF/PE magic bytes by extension-less executables
                    if mime == "application/octet-stream" {
                        ContentClass::Binary
                    } else {
                        ContentClass::Unknown
                    }
                }
            }
        }
        _ => ContentClass::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_rust() {
        assert_eq!(
            classify_mime("text/plain", "src/main.rs"),
            ContentClass::Code {
                language: "rust".into()
            }
        );
    }

    #[test]
    fn test_classify_typescript() {
        assert_eq!(
            classify_mime("text/plain", "app.tsx"),
            ContentClass::Code {
                language: "typescript".into()
            }
        );
    }

    #[test]
    fn test_classify_pdf() {
        assert_eq!(
            classify_mime("application/pdf", "doc.pdf"),
            ContentClass::Pdf
        );
    }

    #[test]
    fn test_classify_image() {
        assert_eq!(classify_mime("image/png", "photo.png"), ContentClass::Image);
    }

    #[test]
    fn test_classify_binary() {
        assert_eq!(
            classify_mime("application/octet-stream", "program"),
            ContentClass::Binary
        );
    }

    #[test]
    fn test_classify_office() {
        assert_eq!(
            classify_mime("application/vnd.openxmlformats", "report.docx"),
            ContentClass::Office
        );
    }

    #[test]
    fn test_classify_nix() {
        assert_eq!(
            classify_mime("text/plain", "flake.nix"),
            ContentClass::Code {
                language: "nix".into()
            }
        );
    }

    #[test]
    fn test_classify_shell() {
        assert_eq!(
            classify_mime("text/plain", "deploy.sh"),
            ContentClass::Code {
                language: "shell".into()
            }
        );
    }

    #[test]
    fn test_node_identity_local() {
        let node = NodeIdentity::local();
        assert!(!node.node_id.is_empty());
        assert_eq!(node.os, std::env::consts::OS);
        assert_eq!(node.arch, std::env::consts::ARCH);
    }

    #[test]
    fn test_build_from_fs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "hello world").unwrap();

        let node = FileNodeBuilder::new(path.to_str().unwrap())
            .build_from_fs()
            .unwrap();

        assert_eq!(node.size, 11);
        assert!(!node.content_hash.is_empty());
        assert_eq!(node.content_class, ContentClass::Text);
        assert!(node.enriched_at.is_none()); // Pass 2 not run
        assert!(node.summary.is_none());
    }
}
