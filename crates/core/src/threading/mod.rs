//! Thread engine — multi-topic conversation threading.
//!
//! Provides conversation threading support that allows a single chat to maintain
//! multiple concurrent topic threads. Each thread has its own history, state, and
//! channel-specific anchoring metadata.
//!
//! # Architecture
//!
//! ```text
//! Message → ThreadRouter → ThreadDecision
//!                              ↓
//!           Existing(id) | New(topic) | Continue
//!                              ↓
//!                        ThreadStore (PluresDB)
//! ```

pub mod auto_archive;
pub mod channel;
pub mod commands;
pub mod compat;
pub mod router;
pub mod store;
pub mod summary;
pub mod types;

pub use auto_archive::AutoArchiver;
pub use channel::{ChannelAnchor, ChannelThreading, ThreadCapabilities, ThreadError};
pub use commands::{SearchResult, ThreadCommand, ThreadCommandExecutor, ThreadCommandResult};
pub use router::ThreadRouter;
pub use store::{MemoryThreadStore, PluresThreadStore, ThreadStore};
pub use summary::{extract_topics, generate_quick_summary};
pub use types::{Thread, ThreadConfig, ThreadDecision, ThreadState};
