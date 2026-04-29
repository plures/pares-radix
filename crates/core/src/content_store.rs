//! Content-addressed storage — deduplicating blob store backed by PluresDB.
//!
//! Large data (files, indexed content, etc.) is stored once by SHA-256 hash
//! and referenced by hash elsewhere in the graph. This is a lightweight stub
//! that can be swapped for `plures-object` when available as a crate.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use pluresdb::CrdtStore;
use serde_json::json;
use sha2::{Digest, Sha256};

/// The PluresDB actor used for content store writes.
const CONTENT_ACTOR: &str = "content-store";

/// Threshold above which content should be stored via ContentStore
/// rather than inline in PluresDB nodes.
pub const LARGE_BLOB_THRESHOLD: usize = 64 * 1024; // 64 KB

/// Content-addressed blob store backed by PluresDB.
pub struct ContentStore {
    store: Arc<CrdtStore>,
}

impl ContentStore {
    /// Create a new content store.
    pub fn new(store: Arc<CrdtStore>) -> Self {
        Self { store }
    }

    /// Hash content without storing it.
    pub fn hash(content: &[u8]) -> String {
        format!("{:x}", Sha256::digest(content))
    }

    /// Store content and return its SHA-256 hash.
    ///
    /// If the content already exists (same hash), this is a no-op.
    pub fn put(&self, content: &[u8]) -> String {
        let hash = Self::hash(content);
        let key = format!("blob:{hash}");

        // Only store if not already present (dedup).
        if self.store.get(&key).is_none() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            self.store.put(
                key,
                CONTENT_ACTOR,
                json!({
                    "_type": "blob",
                    "hash": hash,
                    "size": content.len(),
                    "content": base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        content,
                    ),
                    "stored_at": now,
                }),
            );
        }
        hash
    }

    /// Retrieve content by its hash.
    pub fn get(&self, hash: &str) -> Option<Vec<u8>> {
        let key = format!("blob:{hash}");
        let record = self.store.get(&key)?;
        let b64 = record.data.get("content")?.as_str()?;
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64).ok()
    }

    /// Check if content with the given hash exists.
    pub fn exists(&self, hash: &str) -> bool {
        let key = format!("blob:{hash}");
        self.store.get(&key).is_some()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    fn test_store() -> Arc<CrdtStore> {
        Arc::new(CrdtStore::default())
    }

    #[test]
    fn put_and_get() {
        let store = test_store();
        let cs = ContentStore::new(store);

        let data = b"hello world";
        let hash = cs.put(data);
        assert!(!hash.is_empty());

        let retrieved = cs.get(&hash).expect("should exist");
        assert_eq!(retrieved, data);
    }

    #[test]
    fn dedup() {
        let store = test_store();
        let cs = ContentStore::new(Arc::clone(&store));

        let data = b"duplicate me";
        let h1 = cs.put(data);
        let h2 = cs.put(data);
        assert_eq!(h1, h2);

        // Only one blob node in the store.
        let blob_count = store
            .list()
            .into_iter()
            .filter(|r| r.data.get("_type").and_then(|v| v.as_str()) == Some("blob"))
            .count();
        assert_eq!(blob_count, 1);
    }

    #[test]
    fn exists_check() {
        let store = test_store();
        let cs = ContentStore::new(store);

        assert!(!cs.exists("nonexistent"));
        let hash = cs.put(b"test");
        assert!(cs.exists(&hash));
    }

    #[test]
    fn hash_without_store() {
        let h1 = ContentStore::hash(b"abc");
        let h2 = ContentStore::hash(b"abc");
        assert_eq!(h1, h2);
        assert_ne!(h1, ContentStore::hash(b"def"));
    }
}
