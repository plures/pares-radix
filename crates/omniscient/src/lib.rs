//! # pares-omniscient
//!
//! Two-pass semantic filesystem indexer with cluster-aware cross-system search.
//!
//! ## Architecture
//!
//! ```text
//! ┌─── Pass 1: Extract + Embed (fast, no LLM) ───┐
//! │  inotify → MIME → extract → embed → PluresDB  │
//! └────────────────────────────────────────────────┘
//!           │ async queue
//! ┌─── Pass 2: LLM Enrichment (BitNet) ──────────┐
//! │  summarize → entities → classify → re-embed   │
//! └────────────────────────────────────────────────┘
//! ```
//!
//! Retrieval mirrors the same two-pass pattern:
//! 1. Vector search + graph expansion → candidate set
//! 2. LLM reranking → scored, explained results

pub mod enrichment;
pub mod extractor;
pub mod file_node;
pub mod plugin;
pub mod query;
pub mod scanner;
pub mod store;

pub use file_node::{FileNode, FileNodeBuilder, NodeIdentity};
pub use plugin::{OmniscientConfig, OmniscientPlugin};
pub use query::{Query, QueryConfig, QueryResult};
pub use scanner::{ScanConfig, ScanEvent, Scanner};
