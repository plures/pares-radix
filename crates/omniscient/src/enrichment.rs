//! Pass 2: LLM enrichment pipeline.
//!
//! Takes indexed FileNodes and enriches them with:
//! - Summary (human-readable description)
//! - Entities (people, projects, APIs)
//! - Purpose classification
//! - Relationship detection
//! - Security assessment (for binaries)
//! - Re-embedded vector from enriched content

use crate::file_node::FileNode;
use std::collections::VecDeque;
use tracing::{debug, info};

/// Enrichment request queued for Pass 2.
#[derive(Debug)]
pub struct EnrichmentRequest {
    pub file_node: FileNode,
    pub priority: EnrichmentPriority,
}

/// Priority for enrichment processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnrichmentPriority {
    /// Recently modified files
    High = 0,
    /// New files from full scan
    Normal = 1,
    /// Re-enrichment of stale entries
    Low = 2,
}

/// Result of LLM enrichment.
#[derive(Debug, Clone)]
pub struct EnrichmentResult {
    pub path: String,
    pub node_id: String,
    pub summary: String,
    pub entities: Vec<String>,
    pub purpose: String,
    pub relationships: Vec<String>,
    pub enriched_vector: Option<Vec<f32>>,
}

/// Trait for the LLM backend used for enrichment.
/// Implemented by BitNet (local) and cloud models (fallback).
pub trait EnrichmentBackend: Send + Sync {
    /// Summarize file content.
    fn summarize(
        &self,
        content: &str,
        mime: &str,
        path: &str,
    ) -> Result<EnrichmentResult, Box<dyn std::error::Error + Send + Sync>>;
}

/// The enrichment queue processor.
pub struct EnrichmentPipeline {
    queue: VecDeque<EnrichmentRequest>,
    batch_size: usize,
}

impl EnrichmentPipeline {
    pub fn new(batch_size: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            batch_size,
        }
    }

    /// Add a file to the enrichment queue.
    pub fn enqueue(&mut self, file_node: FileNode, priority: EnrichmentPriority) {
        self.queue.push_back(EnrichmentRequest {
            file_node,
            priority,
        });
        // Keep sorted by priority
        self.queue.make_contiguous().sort_by_key(|r| r.priority);
    }

    /// Process the next batch of enrichment requests.
    pub fn process_batch(
        &mut self,
        backend: &dyn EnrichmentBackend,
    ) -> Vec<EnrichmentResult> {
        let batch: Vec<_> = self.queue
            .drain(..self.batch_size.min(self.queue.len()))
            .collect();

        let mut results = Vec::new();
        for request in batch {
            let content = request.file_node.extracted_text.as_deref().unwrap_or("");
            if content.is_empty() {
                debug!(path = %request.file_node.path, "skipping enrichment: no extracted text");
                continue;
            }

            match backend.summarize(content, &request.file_node.mime, &request.file_node.path) {
                Ok(result) => {
                    info!(path = %request.file_node.path, "enriched");
                    results.push(result);
                }
                Err(e) => {
                    debug!(path = %request.file_node.path, "enrichment failed: {}", e);
                }
            }
        }

        results
    }

    /// Number of items waiting for enrichment.
    pub fn pending(&self) -> usize {
        self.queue.len()
    }
}

/// Build the prompt for Pass 2 LLM enrichment.
pub fn build_enrichment_prompt(content: &str, mime: &str, path: &str) -> String {
    let max_content = if content.len() > 4000 {
        &content[..4000]
    } else {
        content
    };

    format!(
        r#"Analyze this file and provide a structured response.

File: {path}
Type: {mime}

Content (truncated):
```
{max_content}
```

Respond in this exact JSON format:
{{
  "summary": "One-sentence description of what this file does/contains",
  "entities": ["entity1", "entity2"],
  "purpose": "deployment|security|documentation|test|config|library|application|data|other",
  "relationships": ["related_file_or_concept_1", "related_file_or_concept_2"]
}}"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockBackend;

    impl EnrichmentBackend for MockBackend {
        fn summarize(
            &self,
            _content: &str,
            _mime: &str,
            path: &str,
        ) -> Result<EnrichmentResult, Box<dyn std::error::Error + Send + Sync>> {
            Ok(EnrichmentResult {
                path: path.to_string(),
                node_id: "test".into(),
                summary: "A test file".into(),
                entities: vec!["test".into()],
                purpose: "test".into(),
                relationships: vec![],
                enriched_vector: None,
            })
        }
    }

    #[test]
    fn test_enrichment_pipeline() {
        let mut pipeline = EnrichmentPipeline::new(10);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rs");
        std::fs::write(&path, "pub fn main() {}").unwrap();

        let node = crate::file_node::FileNodeBuilder::new(path.to_str().unwrap())
            .build_from_fs()
            .unwrap();

        let mut node_with_text = node;
        node_with_text.extracted_text = Some("pub fn main() {}".into());

        pipeline.enqueue(node_with_text, EnrichmentPriority::Normal);
        assert_eq!(pipeline.pending(), 1);

        let backend = MockBackend;
        let results = pipeline.process_batch(&backend);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].summary, "A test file");
        assert_eq!(pipeline.pending(), 0);
    }

    #[test]
    fn test_priority_ordering() {
        let mut pipeline = EnrichmentPipeline::new(10);

        let dir = tempfile::tempdir().unwrap();

        for (name, priority) in [
            ("low.txt", EnrichmentPriority::Low),
            ("high.txt", EnrichmentPriority::High),
            ("normal.txt", EnrichmentPriority::Normal),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, "content").unwrap();
            let node = crate::file_node::FileNodeBuilder::new(path.to_str().unwrap())
                .build_from_fs()
                .unwrap();
            pipeline.enqueue(node, priority);
        }

        // High should be first
        let first = pipeline.queue.front().unwrap();
        assert!(first.file_node.path.contains("high"));
    }

    #[test]
    fn test_build_prompt() {
        let prompt = build_enrichment_prompt(
            "pub fn deploy() { /* bicep */ }",
            "text/plain",
            "src/deploy.rs",
        );
        assert!(prompt.contains("src/deploy.rs"));
        assert!(prompt.contains("pub fn deploy"));
        assert!(prompt.contains("summary"));
    }
}
