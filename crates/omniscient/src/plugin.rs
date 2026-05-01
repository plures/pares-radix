//! Omniscient plugin runtime — bridges the plugin manifest to the omniscient engine.
//!
//! This is loaded by pares-radix's plugin executor when the omniscient plugin is installed.

use crate::file_node::NodeIdentity;
use crate::scanner::{ScanConfig, Scanner, ScanEvent};
use crate::enrichment::{EnrichmentPipeline, EnrichmentPriority};
use crate::store::{OmniscientStore, MemoryStore, StoreStats};
use crate::query::{QueryConfig, QueryResult};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Plugin configuration (populated from manifest.toml [service] section).
#[derive(Debug, Clone)]
pub struct OmniscientConfig {
    pub watch_paths: Vec<PathBuf>,
    pub ignore_patterns: Vec<String>,
    pub max_file_size_mb: u64,
    pub enrichment_batch_size: usize,
    pub enrichment_interval_seconds: u64,
    pub scan_on_start: bool,
}

impl Default for OmniscientConfig {
    fn default() -> Self {
        Self {
            watch_paths: vec![
                dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")),
            ],
            ignore_patterns: vec![
                ".git".into(), "node_modules".into(), "target".into(),
                ".cache".into(), "__pycache__".into(), "result".into(),
            ],
            max_file_size_mb: 100,
            enrichment_batch_size: 20,
            enrichment_interval_seconds: 300,
            scan_on_start: true,
        }
    }
}

/// The omniscient plugin service.
pub struct OmniscientPlugin {
    config: OmniscientConfig,
    store: Arc<RwLock<Box<dyn OmniscientStore>>>,
    enrichment: Arc<RwLock<EnrichmentPipeline>>,
    node: NodeIdentity,
}

impl OmniscientPlugin {
    pub fn new(config: OmniscientConfig) -> Self {
        let store: Box<dyn OmniscientStore> = Box::new(MemoryStore::new());
        Self {
            config,
            store: Arc::new(RwLock::new(store)),
            enrichment: Arc::new(RwLock::new(EnrichmentPipeline::new(20))),
            node: NodeIdentity::local(),
        }
    }

    /// Start the plugin background service.
    pub async fn start(&self) -> Result<(), String> {
        info!(
            node = %self.node.node_id,
            paths = ?self.config.watch_paths,
            "omniscient plugin starting"
        );

        // Initial scan if configured
        if self.config.scan_on_start {
            self.run_full_scan().await?;
        }

        // Start file watcher
        self.start_watcher().await?;

        // Start enrichment loop
        self.start_enrichment_loop().await;

        Ok(())
    }

    /// Run a full scan of configured paths.
    pub async fn run_full_scan(&self) -> Result<StoreStats, String> {
        let scan_config = ScanConfig {
            watch_paths: self.config.watch_paths.clone(),
            ignore_patterns: self.config.ignore_patterns.clone(),
            max_file_size: self.config.max_file_size_mb * 1024 * 1024,
            node: self.node.clone(),
        };

        let scanner = Scanner::new(scan_config);
        let mut rx = scanner.full_scan().await;

        let store = self.store.clone();
        let enrichment = self.enrichment.clone();

        while let Some(event) = rx.recv().await {
            match event {
                ScanEvent::Indexed(file_node) => {
                    let s = store.read().await;
                    if let Err(e) = s.upsert(&file_node) {
                        warn!(path = %file_node.path, "store error: {}", e);
                    }
                    // Queue for Pass 2 enrichment
                    let mut enrich = enrichment.write().await;
                    enrich.enqueue(*file_node, EnrichmentPriority::Normal);
                }
                ScanEvent::Complete { total, duration_ms } => {
                    info!(total, duration_ms, "full scan complete");
                }
                ScanEvent::Error { path, error } => {
                    warn!(path = %path.display(), error, "scan error");
                }
                _ => {}
            }
        }

        let s = store.read().await;
        s.stats().map_err(|e| e.to_string())
    }

    /// Index a specific directory (triggered by /index command).
    pub async fn index_path(&self, path: PathBuf) -> Result<StoreStats, String> {
        let scan_config = ScanConfig {
            watch_paths: vec![path],
            ignore_patterns: self.config.ignore_patterns.clone(),
            max_file_size: self.config.max_file_size_mb * 1024 * 1024,
            node: self.node.clone(),
        };

        let scanner = Scanner::new(scan_config);
        let mut rx = scanner.full_scan().await;
        let store = self.store.clone();
        let enrichment = self.enrichment.clone();

        while let Some(event) = rx.recv().await {
            if let ScanEvent::Indexed(file_node) = event {
                let s = store.read().await;
                let _ = s.upsert(&file_node);
                let mut enrich = enrichment.write().await;
                enrich.enqueue(*file_node, EnrichmentPriority::High);
            }
        }

        let s = store.read().await;
        s.stats().map_err(|e| e.to_string())
    }

    /// Search the index.
    pub async fn search(&self, _query: &str, _config: QueryConfig) -> Vec<QueryResult> {
        // TODO: embed query with bge-small, search PluresDB vectors,
        // then optionally rerank with BitNet
        //
        // For now, return empty — wiring the actual vector search
        // requires PluresDB integration
        vec![]
    }

    /// Get current stats.
    pub async fn stats(&self) -> Result<StoreStats, String> {
        let s = self.store.read().await;
        s.stats().map_err(|e| e.to_string())
    }

    /// Start the inotify watcher in the background.
    async fn start_watcher(&self) -> Result<(), String> {
        let scan_config = ScanConfig {
            watch_paths: self.config.watch_paths.clone(),
            ignore_patterns: self.config.ignore_patterns.clone(),
            max_file_size: self.config.max_file_size_mb * 1024 * 1024,
            node: self.node.clone(),
        };

        let scanner = Scanner::new(scan_config);
        let mut rx = scanner.watch().await.map_err(|e| e.to_string())?;
        let store = self.store.clone();
        let enrichment = self.enrichment.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    ScanEvent::Indexed(file_node) => {
                        let s = store.read().await;
                        let _ = s.upsert(&file_node);
                        let mut enrich = enrichment.write().await;
                        enrich.enqueue(*file_node, EnrichmentPriority::High);
                    }
                    ScanEvent::Deleted(_path) => {
                        // TODO: delete from store
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    /// Start the enrichment processing loop.
    async fn start_enrichment_loop(&self) {
        let enrichment = self.enrichment.clone();
        let interval_secs = self.config.enrichment_interval_seconds;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;

                let pending = {
                    let e = enrichment.read().await;
                    e.pending()
                };

                if pending > 0 {
                    info!(pending, "enrichment pass starting");
                    // TODO: wire BitNet backend here
                    // let mut e = enrichment.write().await;
                    // let results = e.process_batch(&bitnet_backend);
                    // for result in results { store.update_enrichment(result); }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_creation() {
        let plugin = OmniscientPlugin::new(OmniscientConfig::default());
        let stats = plugin.stats().await.unwrap();
        assert_eq!(stats.total_files, 0);
    }

    #[tokio::test]
    async fn test_index_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "hello world").unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let config = OmniscientConfig {
            watch_paths: vec![],
            scan_on_start: false,
            ..Default::default()
        };
        let plugin = OmniscientPlugin::new(config);
        let stats = plugin.index_path(dir.path().to_path_buf()).await.unwrap();

        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.pending_enrichment, 2); // both awaiting Pass 2
    }

    #[tokio::test]
    async fn test_search_empty() {
        let plugin = OmniscientPlugin::new(OmniscientConfig::default());
        let results = plugin.search("hello", QueryConfig::fast()).await;
        assert!(results.is_empty()); // no files indexed yet
    }
}
