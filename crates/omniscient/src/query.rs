//! Query engine — two-pass retrieval (vector search → LLM reranking).

use serde::{Deserialize, Serialize};

/// A search query.
#[derive(Debug, Clone)]
pub struct Query {
    /// Natural language query
    pub text: String,
    /// Optional filters
    pub config: QueryConfig,
}

/// Query configuration and filters.
#[derive(Debug, Clone, Default)]
pub struct QueryConfig {
    /// Filter by node (e.g., "praxisbot" only)
    pub node_id: Option<String>,
    /// Filter by content class
    pub content_class: Option<String>,
    /// Filter by minimum file size
    pub min_size: Option<u64>,
    /// Filter by maximum file size
    pub max_size: Option<u64>,
    /// Filter by path prefix
    pub path_prefix: Option<String>,
    /// Maximum results from Pass 1
    pub top_k: usize,
    /// Whether to run Pass 2 (LLM reranking)
    pub rerank: bool,
    /// Minimum confidence for Pass 2 results
    pub min_confidence: f32,
}

impl QueryConfig {
    pub fn default_search() -> Self {
        Self {
            top_k: 50,
            rerank: true,
            min_confidence: 0.3,
            ..Default::default()
        }
    }

    pub fn fast() -> Self {
        Self {
            top_k: 20,
            rerank: false,
            min_confidence: 0.0,
            ..Default::default()
        }
    }
}

/// A single search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Which system the file is on
    pub node_id: String,
    /// File path on that system
    pub path: String,
    /// Pass 1 vector similarity score (0-1)
    pub vector_score: f32,
    /// Pass 2 LLM confidence score (0-1), if reranked
    pub confidence: Option<f32>,
    /// LLM explanation of why this matches
    pub explanation: Option<String>,
    /// File summary (from Pass 2 enrichment, if available)
    pub summary: Option<String>,
    /// Content class
    pub content_class: String,
    /// File size
    pub size: u64,
    /// Last modified
    pub modified: String,
}

/// Trait for the vector search backend.
pub trait VectorSearch: Send + Sync {
    /// Search for similar vectors, returning (file_id, score) pairs.
    fn search(&self, vector: &[f32], top_k: usize) -> Vec<(String, f32)>;
}

/// Trait for the LLM reranker (Pass 2 retrieval).
pub trait Reranker: Send + Sync {
    /// Rerank candidates given the original query.
    fn rerank(&self, query: &str, candidates: Vec<RerankCandidate>) -> Vec<RerankResult>;
}

/// Input to the reranker.
#[derive(Debug, Clone)]
pub struct RerankCandidate {
    pub path: String,
    pub node_id: String,
    pub summary: Option<String>,
    pub extracted_text_preview: Option<String>,
    pub vector_score: f32,
}

/// Output from the reranker.
#[derive(Debug, Clone)]
pub struct RerankResult {
    pub path: String,
    pub node_id: String,
    pub confidence: f32,
    pub explanation: String,
}

/// Build the reranking prompt for Pass 2 retrieval.
pub fn build_rerank_prompt(query: &str, candidates: &[RerankCandidate]) -> String {
    let mut candidate_text = String::new();
    for (i, c) in candidates.iter().enumerate() {
        let preview = c
            .summary
            .as_deref()
            .or(c.extracted_text_preview.as_deref())
            .unwrap_or("[no text]");
        let truncated = if preview.len() > 200 {
            &preview[..200]
        } else {
            preview
        };
        candidate_text.push_str(&format!(
            "\n{}. [{}/{}] (score: {:.2})\n   {}\n",
            i + 1,
            c.node_id,
            c.path,
            c.vector_score,
            truncated
        ));
    }

    format!(
        r#"Given this search query, rank the candidates by relevance.
For each relevant candidate, provide a confidence score (0.0-1.0) and a brief explanation.
Only include candidates with confidence > 0.3.

Query: "{query}"

Candidates:
{candidate_text}

Respond as JSON array:
[
  {{"index": 1, "confidence": 0.95, "explanation": "This file directly implements the queried functionality"}},
  ...
]"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_config_defaults() {
        let config = QueryConfig::default_search();
        assert_eq!(config.top_k, 50);
        assert!(config.rerank);
        assert_eq!(config.min_confidence, 0.3);
    }

    #[test]
    fn test_query_config_fast() {
        let config = QueryConfig::fast();
        assert_eq!(config.top_k, 20);
        assert!(!config.rerank);
    }

    #[test]
    fn test_build_rerank_prompt() {
        let candidates = vec![
            RerankCandidate {
                path: "/tmp/deploy.bicep".into(),
                node_id: "praxisbot".into(),
                summary: Some("Bicep template for VM deployment".into()),
                extracted_text_preview: None,
                vector_score: 0.85,
            },
            RerankCandidate {
                path: "/home/user/notes.md".into(),
                node_id: "devbox".into(),
                summary: None,
                extracted_text_preview: Some("Meeting notes about deployment strategy".into()),
                vector_score: 0.72,
            },
        ];

        let prompt = build_rerank_prompt("deployment architecture", &candidates);
        assert!(prompt.contains("deployment architecture"));
        assert!(prompt.contains("deploy.bicep"));
        assert!(prompt.contains("praxisbot"));
        assert!(prompt.contains("devbox"));
    }
}
