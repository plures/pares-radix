//! PluresDB persistence for the procedure-graph repository substrate.
//!
//! `px-repo-model` owns the graph's domain types and canonical BLAKE3
//! hashing. This crate owns only the PluresDB IO boundary: it stores typed
//! node envelopes and explicit edge records, and exposes read APIs over those
//! persisted records. See ADR-0041 for the storage schema.

mod edge;
mod error;
mod node;
mod store;

pub use edge::{EdgeKind, EdgeRecord};
pub use error::GraphStoreError;
pub use node::{GraphNode, GraphNodeKind, NodeKey};
pub use store::GraphStore;
