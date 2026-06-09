//! Cerebellum action handler — IO boundaries for `.px` procedures.
//!
//! This module implements [`AsyncActionHandler`] to provide the side-effect
//! boundary between declarative `.px` procedures (which express cerebellum
//! logic like classification, routing, and context management) and the
//! underlying Rust infrastructure (embedding models, state stores, event bus).
//!
//! # Registered Actions
//!
//! | Action | Params | Returns |
//! |--------|--------|---------|
//! | `compute_embedding` | `{text: string}` | `{embedding: vec<f32>}` |
//! | `cosine_similarity` | `{a: vec<f32>, b: vec<f32>}` | `{similarity: f32}` |
//! | `read_state` | `{key: string}` | `{value: json}` |
//! | `write_state` | `{key: string, value: json}` | `{written: true}` |
//! | `get_current_time` | `{}` | `{timestamp_ms: i64}` |
//! | `emit_event` | `{type: string, payload: json}` | `{emitted: true}` |
//!
//! # Design
//!
//! This is the ONLY Rust code the cerebellum needs for IO — everything else
//! (classification rules, routing decisions, complexity scoring) lives in `.px`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::{mpsc, RwLock};

use crate::memory::embed::EmbeddingProvider;
use crate::px_adapter::AsyncActionHandler;
use crate::spine::event::SpineEvent;
use pares_radix_praxis::px::executor::ExecutionError;

// ── CerebellumActionHandler ──────────────────────────────────────────────────

/// Action handler providing IO boundaries for cerebellum `.px` procedures.
///
/// Each method maps a named action to an async Rust implementation that
/// performs the actual IO (embedding computation, state access, event emission).
/// The `.px` procedures call these by name; this handler is the only bridge.
pub struct CerebellumActionHandler {
    /// Embedding provider for `compute_embedding` action.
    embedder: Option<Arc<dyn EmbeddingProvider>>,
    /// State store for `read_state` / `write_state` actions.
    /// Backed by an in-memory map for now; later migrates to PluresDB.
    state: Arc<RwLock<HashMap<String, Value>>>,
    /// Channel for emitting spine events into the pipeline.
    event_tx: Option<mpsc::Sender<SpineEvent>>,
}

impl CerebellumActionHandler {
    /// Create a new handler with all IO dependencies.
    pub fn new(
        embedder: Option<Arc<dyn EmbeddingProvider>>,
        event_tx: Option<mpsc::Sender<SpineEvent>>,
    ) -> Self {
        Self {
            embedder,
            state: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Create a minimal handler for testing (no embedder, no event channel).
    #[cfg(test)]
    pub fn for_testing() -> Self {
        Self {
            embedder: None,
            state: Arc::new(RwLock::new(HashMap::new())),
            event_tx: None,
        }
    }

    /// Create a minimal handler with no embedder or event channel.
    ///
    /// Useful at startup when the full infrastructure isn't available yet.
    /// Actions that require an embedder will return errors; state operations
    /// work against an in-memory map; events are silently dropped.
    pub fn new_minimal() -> Self {
        Self {
            embedder: None,
            state: Arc::new(RwLock::new(HashMap::new())),
            event_tx: None,
        }
    }

    /// Create a handler with a pre-populated state map (useful for testing).
    #[cfg(test)]
    pub fn with_state(state: HashMap<String, Value>) -> Self {
        Self {
            embedder: None,
            state: Arc::new(RwLock::new(state)),
            event_tx: None,
        }
    }

    // ── Action implementations ───────────────────────────────────────────────

    async fn compute_embedding(&self, params: &Value) -> Result<Value, ExecutionError> {
        let text = params.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
            ExecutionError::ActionFailed {
                action: "compute_embedding".to_string(),
                message: "missing required param: text (string)".to_string(),
            }
        })?;

        let embedder = self
            .embedder
            .as_ref()
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "compute_embedding".to_string(),
                message: "no embedding provider configured".to_string(),
            })?;

        let embedding = embedder
            .embed(text)
            .await
            .map_err(|e| ExecutionError::ActionFailed {
                action: "compute_embedding".to_string(),
                message: e.to_string(),
            })?;

        Ok(json!({ "embedding": embedding }))
    }

    fn cosine_similarity_impl(params: &Value) -> Result<Value, ExecutionError> {
        let a = params.get("a").and_then(|v| v.as_array()).ok_or_else(|| {
            ExecutionError::ActionFailed {
                action: "cosine_similarity".to_string(),
                message: "missing required param: a (array of floats)".to_string(),
            }
        })?;

        let b = params.get("b").and_then(|v| v.as_array()).ok_or_else(|| {
            ExecutionError::ActionFailed {
                action: "cosine_similarity".to_string(),
                message: "missing required param: b (array of floats)".to_string(),
            }
        })?;

        let a_vec: Vec<f32> = a.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect();
        let b_vec: Vec<f32> = b.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect();

        if a_vec.len() != b_vec.len() {
            return Err(ExecutionError::ActionFailed {
                action: "cosine_similarity".to_string(),
                message: format!(
                    "vector dimension mismatch: a={}, b={}",
                    a_vec.len(),
                    b_vec.len()
                ),
            });
        }

        let similarity = cosine_similarity(&a_vec, &b_vec);
        Ok(json!({ "similarity": similarity }))
    }

    async fn read_state(&self, params: &Value) -> Result<Value, ExecutionError> {
        let key = params.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
            ExecutionError::ActionFailed {
                action: "read_state".to_string(),
                message: "missing required param: key (string)".to_string(),
            }
        })?;

        let state = self.state.read().await;
        let value = state.get(key).cloned().unwrap_or(Value::Null);
        Ok(json!({ "value": value }))
    }

    async fn write_state(&self, params: &Value) -> Result<Value, ExecutionError> {
        let key = params.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
            ExecutionError::ActionFailed {
                action: "write_state".to_string(),
                message: "missing required param: key (string)".to_string(),
            }
        })?;

        let value = params.get("value").cloned().unwrap_or(Value::Null);

        let mut state = self.state.write().await;
        state.insert(key.to_string(), value);
        Ok(json!({ "written": true }))
    }

    fn get_current_time() -> Result<Value, ExecutionError> {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ExecutionError::ActionFailed {
                action: "get_current_time".to_string(),
                message: e.to_string(),
            })?
            .as_millis() as i64;

        Ok(json!({ "timestamp_ms": timestamp_ms }))
    }

    async fn emit_event(&self, params: &Value) -> Result<Value, ExecutionError> {
        let event_type = params.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
            ExecutionError::ActionFailed {
                action: "emit_event".to_string(),
                message: "missing required param: type (string)".to_string(),
            }
        })?;

        let payload = params.get("payload").cloned().unwrap_or_else(|| json!({}));

        let tx = self
            .event_tx
            .as_ref()
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "emit_event".to_string(),
                message: "no event channel configured".to_string(),
            })?;

        // Construct a SpineEvent based on the requested type.
        // For now, all cerebellum-emitted events are modelled as ModelRequest
        // (the primary use case is requesting model invocation from .px logic).
        let spine_event = match event_type {
            "model_request" => SpineEvent::ModelRequest {
                id: SpineEvent::new_id(),
                source: "cerebellum".to_string(),
                chat_id: payload
                    .get("chat_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("cerebellum")
                    .to_string(),
                sender: "cerebellum".to_string(),
                content: payload
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                system_prompt: payload
                    .get("system_prompt")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                metadata: payload,
            },
            _ => SpineEvent::Inbound {
                id: SpineEvent::new_id(),
                source: "cerebellum".to_string(),
                chat_id: payload
                    .get("chat_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("cerebellum")
                    .to_string(),
                sender: "cerebellum".to_string(),
                content: json!({ "type": event_type, "payload": payload }).to_string(),
                metadata: json!({ "emitted_by": "cerebellum_action_handler" }),
            },
        };

        tx.send(spine_event)
            .await
            .map_err(|e| ExecutionError::ActionFailed {
                action: "emit_event".to_string(),
                message: format!("failed to send event to pipeline: {e}"),
            })?;

        Ok(json!({ "emitted": true }))
    }
}

#[async_trait]
impl AsyncActionHandler for CerebellumActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "compute_embedding" => self.compute_embedding(params).await,
            "cosine_similarity" => Self::cosine_similarity_impl(params),
            "read_state" => self.read_state(params).await,
            "write_state" => self.write_state(params).await,
            "get_current_time" => Self::get_current_time(),
            "emit_event" => self.emit_event(params).await,
            _ => Err(ExecutionError::UnknownAction(name.to_string())),
        }
    }
}

// ── Pure math ────────────────────────────────────────────────────────────────

/// Compute cosine similarity between two vectors.
///
/// Returns 0.0 for empty or mismatched vectors, and handles zero-magnitude
/// vectors gracefully.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let (dot, norm_a_sq, norm_b_sq) = a
        .iter()
        .zip(b.iter())
        .fold((0.0f32, 0.0f32, 0.0f32), |(dot, na, nb), (&x, &y)| {
            (dot + x * y, na + x * x, nb + y * y)
        });

    let norm_a = norm_a_sq.sqrt();
    let norm_b = norm_b_sq.sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── cosine_similarity tests ──────────────────────────────────────────────

    #[test]
    fn cosine_similarity_identical_vectors() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "identical vectors should have similarity 1.0, got {sim}"
        );
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            sim.abs() < 1e-6,
            "orthogonal vectors should have similarity 0.0, got {sim}"
        );
    }

    #[test]
    fn cosine_similarity_opposite_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            (sim + 1.0).abs() < 1e-6,
            "opposite vectors should have similarity -1.0, got {sim}"
        );
    }

    #[test]
    fn cosine_similarity_known_value() {
        // a = [3, 4], b = [4, 3]
        // dot = 12+12 = 24, |a| = 5, |b| = 5
        // cos = 24/25 = 0.96
        let a = vec![3.0, 4.0];
        let b = vec![4.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.96).abs() < 1e-6, "expected 0.96, got {sim}");
    }

    #[test]
    fn cosine_similarity_empty_vectors() {
        let sim = cosine_similarity(&[], &[]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn cosine_similarity_mismatched_dimensions() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    // ── action dispatch tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn dispatch_unknown_action_returns_error() {
        let handler = CerebellumActionHandler::for_testing();
        let result = handler.call("nonexistent_action", &json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ExecutionError::UnknownAction(name) => assert_eq!(name, "nonexistent_action"),
            other => panic!("expected UnknownAction, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn cosine_similarity_action_dispatch() {
        let handler = CerebellumActionHandler::for_testing();
        let params = json!({
            "a": [1.0, 0.0, 0.0],
            "b": [0.0, 1.0, 0.0]
        });
        let result = handler.call("cosine_similarity", &params).await.unwrap();
        let sim = result["similarity"].as_f64().unwrap();
        assert!(sim.abs() < 1e-6, "orthogonal vectors via action, got {sim}");
    }

    #[tokio::test]
    async fn cosine_similarity_action_dimension_mismatch() {
        let handler = CerebellumActionHandler::for_testing();
        let params = json!({
            "a": [1.0, 2.0],
            "b": [1.0, 2.0, 3.0]
        });
        let result = handler.call("cosine_similarity", &params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn read_state_returns_null_for_missing_key() {
        let handler = CerebellumActionHandler::for_testing();
        let result = handler
            .call("read_state", &json!({"key": "missing"}))
            .await
            .unwrap();
        assert_eq!(result["value"], Value::Null);
    }

    #[tokio::test]
    async fn write_then_read_state() {
        let handler = CerebellumActionHandler::for_testing();

        // Write
        let write_result = handler
            .call("write_state", &json!({"key": "greeting", "value": "hello"}))
            .await
            .unwrap();
        assert_eq!(write_result["written"], true);

        // Read back
        let read_result = handler
            .call("read_state", &json!({"key": "greeting"}))
            .await
            .unwrap();
        assert_eq!(read_result["value"], "hello");
    }

    #[tokio::test]
    async fn write_state_complex_value() {
        let handler = CerebellumActionHandler::for_testing();
        let complex = json!({"nested": {"array": [1, 2, 3]}, "flag": true});

        handler
            .call(
                "write_state",
                &json!({"key": "config", "value": complex.clone()}),
            )
            .await
            .unwrap();

        let result = handler
            .call("read_state", &json!({"key": "config"}))
            .await
            .unwrap();
        assert_eq!(result["value"], complex);
    }

    #[tokio::test]
    async fn get_current_time_returns_reasonable_timestamp() {
        let handler = CerebellumActionHandler::for_testing();
        let result = handler.call("get_current_time", &json!({})).await.unwrap();
        let ts = result["timestamp_ms"].as_i64().unwrap();
        // Should be after 2024-01-01 (1704067200000 ms)
        assert!(
            ts > 1_704_067_200_000,
            "timestamp should be recent, got {ts}"
        );
        // Should be before 2030-01-01 (1893456000000 ms)
        assert!(
            ts < 1_893_456_000_000,
            "timestamp should not be in the far future, got {ts}"
        );
    }

    #[tokio::test]
    async fn emit_event_without_channel_returns_error() {
        let handler = CerebellumActionHandler::for_testing();
        let result = handler
            .call(
                "emit_event",
                &json!({"type": "model_request", "payload": {}}),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn emit_event_sends_to_channel() {
        let (tx, mut rx) = mpsc::channel(16);
        let handler = CerebellumActionHandler::new(None, Some(tx));

        let result = handler
            .call(
                "emit_event",
                &json!({
                    "type": "model_request",
                    "payload": {"chat_id": "test-chat", "content": "hello"}
                }),
            )
            .await
            .unwrap();

        assert_eq!(result["emitted"], true);

        // Verify the event was received
        let event = rx.try_recv().unwrap();
        match event {
            SpineEvent::ModelRequest {
                source,
                chat_id,
                content,
                ..
            } => {
                assert_eq!(source, "cerebellum");
                assert_eq!(chat_id, "test-chat");
                assert_eq!(content, "hello");
            }
            other => panic!("expected ModelRequest, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn compute_embedding_without_provider_returns_error() {
        let handler = CerebellumActionHandler::for_testing();
        let result = handler
            .call("compute_embedding", &json!({"text": "hello world"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn compute_embedding_missing_text_param() {
        let handler = CerebellumActionHandler::for_testing();
        let result = handler.call("compute_embedding", &json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn compute_embedding_with_mock_provider() {
        use crate::memory::embed::MockEmbedder;

        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbedder);
        let handler = CerebellumActionHandler::new(Some(embedder), None);

        let result = handler
            .call("compute_embedding", &json!({"text": "hello world"}))
            .await
            .unwrap();

        let embedding = result["embedding"].as_array().unwrap();
        assert_eq!(embedding.len(), 384); // MockEmbedder uses EMBEDDING_DIM = 384
    }

    #[tokio::test]
    async fn read_state_missing_key_param() {
        let handler = CerebellumActionHandler::for_testing();
        let result = handler.call("read_state", &json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn write_state_missing_key_param() {
        let handler = CerebellumActionHandler::for_testing();
        let result = handler.call("write_state", &json!({"value": 42})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn emit_event_missing_type_param() {
        let (tx, _rx) = mpsc::channel(16);
        let handler = CerebellumActionHandler::new(None, Some(tx));
        let result = handler.call("emit_event", &json!({"payload": {}})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn emit_event_generic_type_creates_inbound() {
        let (tx, mut rx) = mpsc::channel(16);
        let handler = CerebellumActionHandler::new(None, Some(tx));

        let result = handler
            .call(
                "emit_event",
                &json!({
                    "type": "custom_event",
                    "payload": {"data": "test"}
                }),
            )
            .await
            .unwrap();

        assert_eq!(result["emitted"], true);

        let event = rx.try_recv().unwrap();
        matches!(event, SpineEvent::Inbound { .. });
    }
}
