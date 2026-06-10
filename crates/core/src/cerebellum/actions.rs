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
use std::sync::RwLock as StdRwLock;

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
    /// Model client for `model_complete` action.
    /// Wrapped in RwLock so it can be set after construction (late binding).
    model_client: Arc<StdRwLock<Option<Arc<dyn crate::model::ModelClient>>>>,
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
            model_client: Arc::new(StdRwLock::new(None)),
        }
    }

    /// Create a minimal handler for testing (no embedder, no event channel).
    #[cfg(test)]
    pub fn for_testing() -> Self {
        Self {
            embedder: None,
            state: Arc::new(RwLock::new(HashMap::new())),
            event_tx: None,
            model_client: Arc::new(StdRwLock::new(None)),
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
            model_client: Arc::new(StdRwLock::new(None)),
        }
    }

    /// Attach a model client to enable `model_complete` action.
    /// Can be called after construction (late binding pattern).
    pub fn with_model_client(self, client: Arc<dyn crate::model::ModelClient>) -> Self {
        *self.model_client.write().unwrap() = Some(client);
        self
    }

    /// Set the model client after construction (for late binding when
    /// the model client isn't available at cerebellum init time).
    pub fn set_model_client(&self, client: Arc<dyn crate::model::ModelClient>) {
        *self.model_client.write().unwrap() = Some(client);
    }

    /// Create a handler with a pre-populated state map (useful for testing).
    #[cfg(test)]
    pub fn with_state(state: HashMap<String, Value>) -> Self {
        Self {
            embedder: None,
            state: Arc::new(RwLock::new(state)),
            event_tx: None,
            model_client: Arc::new(StdRwLock::new(None)),
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

    // ── Dataflow classification actions ───────────────────────────────────────

    /// Normalize text: lowercase, trim whitespace.
    fn normalize_text(params: &Value) -> Result<Value, ExecutionError> {
        let text = params["text"].as_str().unwrap_or_default();
        Ok(json!(text.to_lowercase().trim().to_string()))
    }

    /// Detect intent from text: question, command, statement, greeting, farewell.
    fn detect_intent(params: &Value) -> Result<Value, ExecutionError> {
        let text = params["text"].as_str().unwrap_or_default();
        let intent = if text.ends_with('?') || text.starts_with("what ") || text.starts_with("how ")
            || text.starts_with("why ") || text.starts_with("when ") || text.starts_with("where ")
            || text.starts_with("who ") || text.starts_with("can you")
        {
            "question"
        } else if text.starts_with('/') || text.starts_with("do ") || text.starts_with("run ")
            || text.starts_with("execute ") || text.starts_with("create ")
            || text.starts_with("make ") || text.starts_with("build ")
            || text.starts_with("deploy ") || text.starts_with("fix ")
        {
            "command"
        } else if text.starts_with("hi") || text.starts_with("hey") || text.starts_with("hello") {
            "greeting"
        } else if text.starts_with("bye") || text.starts_with("goodbye") || text.starts_with("see you") {
            "farewell"
        } else {
            "statement"
        };
        Ok(json!(intent))
    }

    /// Score complexity 0-6 based on structural cues.
    fn score_complexity(params: &Value) -> Result<Value, ExecutionError> {
        let text = params["text"].as_str().unwrap_or_default();
        let words: Vec<&str> = text.split_whitespace().collect();
        let word_count = words.len();
        let mut score: u32 = 0;

        // Length factor
        if word_count > 30 {
            score += 2;
        } else if word_count > 8 {
            score += 1;
        }

        // Reasoning words
        let reasoning = ["because", "therefore", "however", "although", "whereas",
            "analyze", "compare", "evaluate", "explain", "consider"];
        if words.iter().any(|w| reasoning.contains(&w.to_lowercase().as_str())) {
            score += 1;
        }

        // Multi-step markers
        let step_markers = ["first", "then", "next", "finally", "after", "before",
            "step", "1.", "2.", "3."];
        let step_count = words.iter().filter(|w| step_markers.contains(&w.to_lowercase().as_str())).count();
        if step_count >= 2 {
            score += 1;
        }

        // Code markers
        if text.contains('`') || text.contains("fn ") || text.contains("def ")
            || text.contains("->") || text.contains("::") || text.contains("impl ")
        {
            score += 1;
        }

        // Multi-clause
        let clauses = text.matches(',').count() + text.matches(';').count()
            + text.matches(" and ").count() + text.matches(" or ").count();
        if clauses >= 3 {
            score += 1;
        }

        Ok(json!(score.min(6)))
    }

    /// Detect if tools are needed based on text patterns.
    fn detect_tools_needed(params: &Value) -> Result<Value, ExecutionError> {
        let text = params["text"].as_str().unwrap_or_default();
        let needs_tools = text.contains("search") || text.contains("browse")
            || text.contains("fetch") || text.contains("download")
            || text.contains("run ") || text.contains("execute")
            || text.contains("compile") || text.contains("build")
            || text.contains("deploy") || text.contains("commit")
            || text.contains("push") || text.contains("pull")
            || text.starts_with('/');
        Ok(json!(needs_tools))
    }

    /// Match against known plugin/tool patterns.
    fn match_plugin(params: &Value) -> Result<Value, ExecutionError> {
        let text = params["text"].as_str().unwrap_or_default();
        let plugin = if text.contains("weather") {
            "weather"
        } else if text.contains("calendar") || text.contains("schedule") {
            "calendar"
        } else if text.contains("email") || text.contains("mail") {
            "email"
        } else if text.contains("git") || text.contains("repo") || text.contains("pr ") {
            "git"
        } else if text.contains("memory") || text.contains("remember") {
            "memory"
        } else {
            "none"
        };
        Ok(json!(plugin))
    }

    /// Extract topic from text (first noun phrase heuristic).
    fn extract_topic(params: &Value) -> Result<Value, ExecutionError> {
        let text = params["text"].as_str().unwrap_or_default();
        // Simple: take the first 3-5 significant words
        let stop_words = ["the", "a", "an", "is", "are", "was", "were", "do", "does",
            "did", "to", "of", "in", "on", "at", "for", "with", "and", "or", "but",
            "can", "you", "i", "me", "my", "it", "this", "that"];
        let significant: Vec<&str> = text.split_whitespace()
            .filter(|w| !stop_words.contains(&w.to_lowercase().as_str()))
            .take(4)
            .collect();
        Ok(json!(significant.join(" ")))
    }

    /// Detect topic shift (placeholder — needs embedding comparison).
    fn detect_topic_shift_action(params: &Value) -> Result<Value, ExecutionError> {
        // Without embeddings, assume no shift (conservative)
        let _topic = params["topic"].as_str().unwrap_or_default();
        Ok(json!(false))
    }

    /// Determine model tier based on complexity score.
    fn determine_model_tier(params: &Value) -> Result<Value, ExecutionError> {
        let complexity = params["complexity"].as_u64().unwrap_or(0);
        let needs_deep = complexity > 3;
        Ok(json!(needs_deep))
    }

    /// Generic classify action (combines intent + complexity + tools).
    fn classify_action(params: &Value) -> Result<Value, ExecutionError> {
        let text = params["text"].as_str().unwrap_or_default();
        let intent = Self::detect_intent(&json!({"text": text}))?;
        let complexity = Self::score_complexity(&json!({"text": text}))?;
        let needs_tools = Self::detect_tools_needed(&json!({"text": text}))?;
        Ok(json!({
            "intent": intent,
            "complexity": complexity,
            "needs_tools": needs_tools,
        }))
    }

    /// Call the model client with messages and return the completion.
    async fn model_complete_action(&self, params: &Value) -> Result<Value, ExecutionError> {
        let client = self.model_client.read().unwrap().clone().ok_or_else(|| {
            ExecutionError::ActionFailed {
                action: "model_complete".to_string(),
                message: "no model client attached (call set_model_client first)".to_string(),
            }
        })?;

        // Extract messages from params
        let messages_raw = params.get("messages").cloned().unwrap_or(json!([]));
        let system_prompt = params.get("system_prompt")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let tier = params.get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or("standard");

        // Build ChatMessage list
        use crate::model::{ChatMessage, ChatOptions};
        let mut chat_messages: Vec<ChatMessage> = vec![];

        if !system_prompt.is_empty() {
            chat_messages.push(ChatMessage::system(system_prompt));
        }

        // Parse raw messages array
        if let Some(arr) = messages_raw.as_array() {
            for msg in arr {
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                let content = msg.get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or_default()
                    .to_string();
                match role {
                    "system" => chat_messages.push(ChatMessage::system(content)),
                    "assistant" => chat_messages.push(ChatMessage::assistant(content)),
                    _ => chat_messages.push(ChatMessage::user(content)),
                }
            }
        }

        let options = ChatOptions {
            temperature: match tier {
                "premium" => Some(0.7),
                "fast" => Some(0.3),
                _ => Some(0.5),
            },
            ..Default::default()
        };

        match client.complete(&chat_messages, &[], &options).await {
            Ok(completion) => Ok(json!({
                "content": completion.content,
                "model": completion.model,
                "tier": tier,
            })),
            Err(e) => Err(ExecutionError::ActionFailed {
                action: "model_complete".to_string(),
                message: e,
            }),
        }
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
            // Dataflow classification actions
            "normalize_text" => Self::normalize_text(params),
            "detect_intent" => Self::detect_intent(params),
            "score_complexity" => Self::score_complexity(params),
            "detect_tools_needed" => Self::detect_tools_needed(params),
            "match_plugin" => Self::match_plugin(params),
            "extract_topic" => Self::extract_topic(params),
            "detect_topic_shift" => Self::detect_topic_shift_action(params),
            "determine_model_tier" => Self::determine_model_tier(params),
            "classify" => Self::classify_action(params),
            "model_complete" => self.model_complete_action(params).await,
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
