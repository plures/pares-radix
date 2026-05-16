//! Filesystem scanner — watches for changes and queues extraction.

use crate::extractor;
use crate::file_node::{FileNode, FileNodeBuilder, NodeIdentity};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Configuration for the scanner.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Directories to watch
    pub watch_paths: Vec<PathBuf>,
    /// File patterns to ignore (e.g., ".git", "node_modules", "target")
    pub ignore_patterns: Vec<String>,
    /// Maximum file size to index (default: 100MB)
    pub max_file_size: u64,
    /// Node identity for this system
    pub node: NodeIdentity,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            watch_paths: vec![],
            ignore_patterns: vec![
                ".git".into(),
                "node_modules".into(),
                "target".into(),
                ".cache".into(),
                "__pycache__".into(),
                ".nix-profile".into(),
                ".local/share/Trash".into(),
                "result".into(),
            ],
            max_file_size: 100 * 1024 * 1024, // 100MB
            node: NodeIdentity::local(),
        }
    }
}

/// Events emitted by the scanner.
#[derive(Debug)]
pub enum ScanEvent {
    /// A file was indexed (Pass 1 complete)
    Indexed(Box<FileNode>),
    /// A file was modified and needs re-indexing
    Modified(PathBuf),
    /// A file was deleted
    Deleted(PathBuf),
    /// Scan progress
    Progress { scanned: usize, total: usize },
    /// Scan complete
    Complete { total: usize, duration_ms: u64 },
    /// Error during scanning
    Error { path: PathBuf, error: String },
}

/// The filesystem scanner.
pub struct Scanner {
    config: ScanConfig,
}

impl Scanner {
    pub fn new(config: ScanConfig) -> Self {
        Self { config }
    }

    /// Perform a full scan of all configured paths.
    /// Returns a channel that receives ScanEvents as files are processed.
    pub async fn full_scan(&self) -> mpsc::Receiver<ScanEvent> {
        let (tx, rx) = mpsc::channel(1000);
        let config = self.config.clone();

        tokio::spawn(async move {
            let start = std::time::Instant::now();
            let mut total = 0;

            for watch_path in &config.watch_paths {
                if !watch_path.exists() {
                    let _ = tx
                        .send(ScanEvent::Error {
                            path: watch_path.clone(),
                            error: "path does not exist".into(),
                        })
                        .await;
                    continue;
                }

                let files =
                    collect_files(watch_path, &config.ignore_patterns, config.max_file_size);
                let file_count = files.len();
                info!(path = %watch_path.display(), count = file_count, "scanning directory");

                for file_path in files {
                    match index_file(&file_path, &config.node) {
                        Ok(node) => {
                            let _ = tx.send(ScanEvent::Indexed(Box::new(node))).await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(ScanEvent::Error {
                                    path: file_path,
                                    error: e.to_string(),
                                })
                                .await;
                        }
                    }

                    total += 1;
                    if total % 100 == 0 {
                        let _ = tx
                            .send(ScanEvent::Progress {
                                scanned: total,
                                total: file_count,
                            })
                            .await;
                    }
                }
            }

            let duration_ms = start.elapsed().as_millis() as u64;
            let _ = tx.send(ScanEvent::Complete { total, duration_ms }).await;
        });

        rx
    }

    /// Start watching for filesystem changes (inotify/fanotify).
    /// Returns a channel that receives events for changed files.
    pub async fn watch(&self) -> Result<mpsc::Receiver<ScanEvent>, std::io::Error> {
        let (tx, rx) = mpsc::channel(1000);
        let config = self.config.clone();

        tokio::spawn(async move {
            use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

            let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<Event>(1000);

            let mut watcher = match RecommendedWatcher::new(
                move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res {
                        let _ = notify_tx.blocking_send(event);
                    }
                },
                notify::Config::default(),
            ) {
                Ok(w) => w,
                Err(e) => {
                    warn!("failed to create watcher: {}", e);
                    return;
                }
            };

            for path in &config.watch_paths {
                if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
                    warn!(path = %path.display(), "failed to watch: {}", e);
                }
            }

            info!("watching {} paths for changes", config.watch_paths.len());

            while let Some(event) = notify_rx.recv().await {
                for path in event.paths {
                    if should_ignore(&path, &config.ignore_patterns) {
                        continue;
                    }

                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            match index_file(&path, &config.node) {
                                Ok(node) => {
                                    let _ = tx.send(ScanEvent::Indexed(Box::new(node))).await;
                                }
                                Err(e) => {
                                    debug!(path = %path.display(), "skip: {}", e);
                                }
                            }
                        }
                        EventKind::Remove(_) => {
                            let _ = tx.send(ScanEvent::Deleted(path)).await;
                        }
                        _ => {}
                    }
                }
            }
        });

        Ok(rx)
    }
}

/// Index a single file (Pass 1).
fn index_file(path: &Path, node: &NodeIdentity) -> Result<FileNode, String> {
    let path_str = path.to_str().ok_or("invalid path")?;
    let mut file_node = FileNodeBuilder::new(path_str)
        .with_node(node.clone())
        .build_from_fs()
        .map_err(|e| e.to_string())?;

    // Read content and extract
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let extraction = extractor::extract(&file_node.content_class, &data, path_str);

    file_node.extracted_text = extraction.text;
    file_node.security = extraction.security;

    // TODO: embed extracted_text with bge-small → file_node.raw_vector

    Ok(file_node)
}

/// Collect all files under a directory, respecting ignore patterns and size limits.
fn collect_files(root: &Path, ignore: &[String], max_size: u64) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let ignore_set: HashSet<&str> = ignore.iter().map(|s| s.as_str()).collect();

    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            !ignore_set.contains(name)
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() {
            continue;
        }

        if let Ok(meta) = entry.metadata() {
            if meta.len() <= max_size {
                files.push(entry.into_path());
            }
        }
    }

    files
}

fn should_ignore(path: &Path, patterns: &[String]) -> bool {
    let path_str = path.to_str().unwrap_or("");
    patterns.iter().any(|p| path_str.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_files_ignores_patterns() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create files
        std::fs::write(root.join("hello.txt"), "hello").unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/config"), "gitconfig").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

        let files = collect_files(root, &[".git".into()], 100 * 1024 * 1024);
        let names: Vec<String> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert!(names.contains(&"hello.txt".to_string()));
        assert!(names.contains(&"main.rs".to_string()));
        assert!(!names.contains(&"config".to_string())); // .git ignored
    }

    #[test]
    fn test_collect_files_respects_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::write(root.join("small.txt"), "small").unwrap();
        std::fs::write(root.join("big.txt"), "x".repeat(1000)).unwrap();

        let files = collect_files(root, &[], 100); // 100 byte limit
        let names: Vec<String> = files
            .iter()
            .map(|f| f.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert!(names.contains(&"small.txt".to_string()));
        assert!(!names.contains(&"big.txt".to_string()));
    }

    #[test]
    fn test_should_ignore() {
        assert!(should_ignore(
            Path::new("/home/user/.git/config"),
            &[".git".into()]
        ));
        assert!(should_ignore(
            Path::new("/tmp/project/node_modules/pkg/index.js"),
            &["node_modules".into()]
        ));
        assert!(!should_ignore(
            Path::new("/tmp/project/src/main.rs"),
            &[".git".into()]
        ));
    }

    #[test]
    fn test_index_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rs");
        std::fs::write(&path, "pub fn main() {\n    println!(\"hello\");\n}\n").unwrap();

        let node = NodeIdentity::local();
        let file_node = index_file(&path, &node).unwrap();

        assert!(file_node.extracted_text.is_some());
        assert!(file_node.extracted_text.unwrap().contains("pub fn main"));
        assert_eq!(file_node.node.node_id, node.node_id);
    }
}
