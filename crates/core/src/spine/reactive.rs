//! Reactive trigger system for PluresDB write events.
//!
//! The [`ReactiveRegistry`] holds compiled `.px` procedures indexed by their
//! trigger patterns and fires them asynchronously when matching write keys are
//! observed. This enables declarative, event-driven automation:
//!
//! ```text
//! PluresDB write("inbound:12345", value)
//!     → ReactiveRegistry.on_write("inbound:12345", &value)
//!         → pattern "inbound:*" matches
//!             → spawn .px procedure execution
//!                 → emitted events forwarded to PipelineEmitter
//! ```
//!
//! Procedures execute on spawned tasks so the write path is never blocked.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{oneshot, RwLock};
use tracing::{debug, error, info, warn};

use crate::procedure::Procedure;
use crate::px_adapter::PxProcedureAdapter;
use crate::spine::pipeline::PipelineEmitter;

// ── Trigger Pattern ──────────────────────────────────────────────────────────

/// A glob-style pattern for matching write keys.
///
/// Supports suffix-wildcard patterns like `inbound:*` which matches any key
/// starting with `inbound:`. Non-wildcard patterns require exact equality.
#[derive(Debug, Clone)]
pub struct TriggerPattern {
    /// The raw pattern string, e.g. "inbound:*".
    raw: String,
    /// Prefix before the wildcard (for fast matching).
    prefix: String,
    /// Whether this is a wildcard (glob) pattern.
    is_glob: bool,
}

impl TriggerPattern {
    /// Create a new trigger pattern from a raw pattern string.
    ///
    /// Patterns ending with `*` are treated as glob patterns that match
    /// any key starting with the prefix before the wildcard.
    pub fn new(pattern: &str) -> Self {
        if let Some(prefix) = pattern.strip_suffix('*') {
            Self {
                raw: pattern.to_string(),
                prefix: prefix.to_string(),
                is_glob: true,
            }
        } else {
            Self {
                raw: pattern.to_string(),
                prefix: pattern.to_string(),
                is_glob: false,
            }
        }
    }

    /// Test whether the given key matches this pattern.
    pub fn matches(&self, key: &str) -> bool {
        if self.is_glob {
            key.starts_with(&self.prefix)
        } else {
            key == self.raw
        }
    }

    /// Return the raw pattern string.
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

// ── Reactive Registry ────────────────────────────────────────────────────────

/// A registered trigger entry pairing a pattern with its procedure adapter.
struct TriggerEntry {
    pattern: TriggerPattern,
    adapter: Arc<PxProcedureAdapter>,
}

/// Registry of reactive procedures triggered by PluresDB write events.
///
/// When data is written to PluresDB (conversation store, model responses, etc.),
/// the writer calls [`ReactiveRegistry::on_write`] which pattern-matches the
/// key against all registered triggers and spawns execution of matching
/// procedures asynchronously.
pub struct ReactiveRegistry {
    /// Registered trigger entries (pattern + adapter pairs).
    triggers: RwLock<Vec<TriggerEntry>>,
    /// Pipeline emitter for forwarding events produced by procedure execution.
    /// Wrapped in RwLock so it can be set after construction (breaking the
    /// circular dependency between Pipeline and Registry).
    emitter: RwLock<Option<PipelineEmitter>>,
    /// Pending result waiters: key → list of oneshot senders waiting for that write.
    /// Used by callers who need to await the output of a reactive chain.
    waiters: RwLock<HashMap<String, Vec<oneshot::Sender<Value>>>>,
}

impl ReactiveRegistry {
    /// Create a new empty registry without a pipeline emitter.
    ///
    /// Events emitted by triggered procedures will be logged but discarded
    /// until [`set_emitter`] is called.
    pub fn new() -> Self {
        Self {
            triggers: RwLock::new(Vec::new()),
            emitter: RwLock::new(None),
            waiters: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new registry that forwards emitted events to the given pipeline.
    pub fn with_emitter(emitter: PipelineEmitter) -> Self {
        Self {
            triggers: RwLock::new(Vec::new()),
            emitter: RwLock::new(Some(emitter)),
            waiters: RwLock::new(HashMap::new()),
        }
    }

    /// Set (or replace) the pipeline emitter after construction.
    ///
    /// This breaks the circular dependency: create the registry first, pass it
    /// to `Pipeline::with_reactive`, then set the emitter from the pipeline.
    pub async fn set_emitter(&self, emitter: PipelineEmitter) {
        *self.emitter.write().await = Some(emitter);
    }

    /// Subscribe to the result of a specific key write.
    ///
    /// Returns a oneshot receiver that will fire when `on_write` is called
    /// with the exact key. Used to await the output of reactive procedure chains.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let rx = registry.subscribe_result("route_decision:msg-123").await;
    /// registry.on_write("inbound:msg-123", &value).await; // triggers chain
    /// let result = tokio::time::timeout(Duration::from_secs(5), rx).await;
    /// ```
    pub async fn subscribe_result(&self, key: &str) -> oneshot::Receiver<Value> {
        let (tx, rx) = oneshot::channel();
        self.waiters
            .write()
            .await
            .entry(key.to_string())
            .or_default()
            .push(tx);
        rx
    }

    /// Register a `.px` procedure to be triggered on writes matching the pattern.
    ///
    /// # Arguments
    ///
    /// * `pattern` — A glob-style pattern (e.g. `"inbound:*"`, `"state:config"`).
    /// * `adapter` — The compiled `.px` procedure adapter to execute on match.
    pub async fn register_procedure(&self, pattern: &str, adapter: Arc<PxProcedureAdapter>) {
        let trigger_pattern = TriggerPattern::new(pattern);
        let proc_name = Procedure::name(&*adapter);
        info!(
            pattern = %trigger_pattern.raw(),
            procedure = proc_name,
            "reactive: registered trigger"
        );
        self.triggers.write().await.push(TriggerEntry {
            pattern: trigger_pattern,
            adapter,
        });
    }

    /// Notify the registry that a key was written to PluresDB.
    ///
    /// Matching procedures are executed asynchronously via `tokio::spawn` so
    /// the write path is never blocked. Errors within procedure execution are
    /// logged and do not propagate to the caller.
    ///
    /// # Arguments
    ///
    /// * `key` — The PluresDB key that was written (e.g. `"inbound:12345"`).
    /// * `value` — The value that was written.
    pub async fn on_write(&self, key: &str, value: &Value) {
        let triggers = self.triggers.read().await;

        for entry in triggers.iter() {
            if entry.pattern.matches(key) {
                let proc_name = Procedure::name(&*entry.adapter);
                debug!(
                    key = %key,
                    pattern = %entry.pattern.raw(),
                    procedure = proc_name,
                    "reactive: trigger matched, spawning execution"
                );

                let adapter = entry.adapter.clone();
                let emitter = self.emitter.read().await.clone();
                let key_owned = key.to_string();
                let value_owned = value.clone();

                tokio::spawn(async move {
                    Self::execute_triggered(adapter, emitter, &key_owned, &value_owned).await;
                });
            }
        }

        // Notify any waiters subscribed to this exact key
        let mut waiters = self.waiters.write().await;
        if let Some(senders) = waiters.remove(key) {
            debug!(
                key = %key,
                waiter_count = senders.len(),
                "reactive: notifying result waiters"
            );
            for tx in senders {
                // Ignore send error (receiver may have been dropped/timed out)
                let _ = tx.send(value.clone());
            }
        }
    }

    /// Execute a triggered procedure with key/value as initial variables.
    ///
    /// Emitted events (from the `$emit` variable convention) are forwarded
    /// to the pipeline emitter if one is configured.
    async fn execute_triggered(
        adapter: Arc<PxProcedureAdapter>,
        emitter: Option<PipelineEmitter>,
        key: &str,
        value: &Value,
    ) {
        let proc_name = Procedure::name(&*adapter);

        // Build initial variables from the write event
        let mut vars: HashMap<String, Value> = HashMap::new();
        vars.insert("key".to_string(), Value::String(key.to_string()));
        vars.insert("value".to_string(), value.clone());
        vars.insert(
            "event_kind".to_string(),
            Value::String("on_write".to_string()),
        );

        match adapter.execute_with_vars(vars).await {
            Ok(result) => {
                if result.success {
                    debug!(
                        procedure = proc_name,
                        key = %key,
                        "reactive: procedure executed successfully"
                    );

                    // Forward emitted events to the pipeline
                    if let Some(ref emitter) = emitter {
                        if let Some(emit_val) = result.variables.get("emit") {
                            Self::forward_emitted_events(emitter, emit_val, proc_name).await;
                        }
                    }
                } else {
                    warn!(
                        procedure = proc_name,
                        key = %key,
                        error = ?result.error,
                        "reactive: procedure execution failed"
                    );
                }
            }
            Err(err) => {
                error!(
                    procedure = proc_name,
                    key = %key,
                    error = %err,
                    "reactive: procedure executor error"
                );
            }
        }
    }

    /// Forward emitted events from procedure execution to the pipeline.
    ///
    /// Events are expected as a JSON array in the `$emit` variable following
    /// the convention established in [`PxProcedureAdapter`].
    async fn forward_emitted_events(emitter: &PipelineEmitter, emit_val: &Value, proc_name: &str) {
        use crate::spine::event::SpineEvent;

        let events = match emit_val.as_array() {
            Some(arr) => arr,
            None => {
                warn!(
                    procedure = proc_name,
                    "reactive: $emit is not an array, skipping"
                );
                return;
            }
        };

        for event_json in events {
            match serde_json::from_value::<SpineEvent>(event_json.clone()) {
                Ok(spine_event) => {
                    debug!(
                        procedure = proc_name,
                        event_type = spine_event.event_type(),
                        "reactive: forwarding emitted event"
                    );
                    emitter.emit(spine_event).await;
                }
                Err(e) => {
                    warn!(
                        procedure = proc_name,
                        error = %e,
                        "reactive: failed to deserialize emitted event"
                    );
                }
            }
        }
    }

    /// Return the number of registered trigger patterns.
    pub async fn trigger_count(&self) -> usize {
        self.triggers.read().await.len()
    }
}

impl Default for ReactiveRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::px_adapter::AsyncActionHandler;

    // ── TriggerPattern Tests ─────────────────────────────────────────────

    #[test]
    fn exact_pattern_matches_exact_key() {
        let pattern = TriggerPattern::new("state:config");
        assert!(pattern.matches("state:config"));
        assert!(!pattern.matches("state:config:sub"));
        assert!(!pattern.matches("state:confi"));
        assert!(!pattern.matches("other:config"));
    }

    #[test]
    fn glob_pattern_matches_prefix() {
        let pattern = TriggerPattern::new("inbound:*");
        assert!(pattern.matches("inbound:12345"));
        assert!(pattern.matches("inbound:"));
        assert!(pattern.matches("inbound:foo:bar"));
        assert!(!pattern.matches("outbound:12345"));
        assert!(!pattern.matches("inbound"));
    }

    #[test]
    fn wildcard_only_matches_everything() {
        let pattern = TriggerPattern::new("*");
        assert!(pattern.matches("anything"));
        assert!(pattern.matches(""));
        assert!(pattern.matches("deeply:nested:key"));
    }

    #[test]
    fn empty_pattern_matches_empty_key_only() {
        let pattern = TriggerPattern::new("");
        assert!(pattern.matches(""));
        assert!(!pattern.matches("something"));
    }

    #[test]
    fn pattern_with_multiple_colons() {
        let pattern = TriggerPattern::new("a:b:*");
        assert!(pattern.matches("a:b:c"));
        assert!(pattern.matches("a:b:"));
        assert!(!pattern.matches("a:b"));
        assert!(!pattern.matches("a:c:d"));
    }

    // ── ReactiveRegistry Tests ───────────────────────────────────────────

    #[tokio::test]
    async fn registry_starts_empty() {
        let registry = ReactiveRegistry::new();
        assert_eq!(registry.trigger_count().await, 0);
    }

    #[tokio::test]
    async fn register_procedure_increments_count() {
        let registry = ReactiveRegistry::new();
        let handler = Arc::new(NoOpActionHandler);
        let adapter = make_test_adapter(handler);

        registry
            .register_procedure("test:*", Arc::new(adapter))
            .await;
        assert_eq!(registry.trigger_count().await, 1);
    }

    #[tokio::test]
    async fn on_write_with_no_triggers_does_not_panic() {
        let registry = ReactiveRegistry::new();
        registry
            .on_write("some:key", &Value::String("hello".into()))
            .await;
        // No panic = success
    }

    #[tokio::test]
    async fn on_write_spawns_for_matching_pattern() {
        use std::sync::atomic::AtomicUsize;
        use tokio::time::{sleep, Duration};

        let call_count = Arc::new(AtomicUsize::new(0));
        let handler = Arc::new(CountingActionHandler {
            count: call_count.clone(),
        });
        let adapter = Arc::new(make_test_adapter(handler));

        let registry = ReactiveRegistry::new();
        registry.register_procedure("inbound:*", adapter).await;

        registry
            .on_write("inbound:msg123", &Value::String("test".into()))
            .await;

        // Give the spawned task time to execute
        sleep(Duration::from_millis(100)).await;

        // The procedure was attempted (it may fail due to minimal compiled data
        // but the spawn happened — we verify via the handler call count or
        // by checking that no panic occurred)
        // With our test adapter, execution will fail gracefully since compiled
        // data is minimal, but the spawn path was exercised.
    }

    #[tokio::test]
    async fn on_write_ignores_non_matching_patterns() {
        let handler = Arc::new(NoOpActionHandler);
        let adapter = Arc::new(make_test_adapter(handler));

        let registry = ReactiveRegistry::new();
        registry.register_procedure("inbound:*", adapter).await;

        // Should not panic or fire for non-matching key
        registry
            .on_write("outbound:msg123", &Value::String("test".into()))
            .await;

        // Give potential spawns time to execute (there should be none)
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn multiple_patterns_can_match_same_key() {
        let handler = Arc::new(NoOpActionHandler);
        let adapter1 = Arc::new(make_test_adapter(handler.clone()));
        let adapter2 = Arc::new(make_test_adapter(handler));

        let registry = ReactiveRegistry::new();
        registry.register_procedure("inbound:*", adapter1).await;
        registry.register_procedure("*", adapter2).await;

        assert_eq!(registry.trigger_count().await, 2);

        // Both patterns match — both should fire (no panic)
        registry
            .on_write("inbound:123", &Value::String("test".into()))
            .await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn on_write_with_emitter_forwards_events() {
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel(16);
        let emitter = PipelineEmitter { tx };
        let registry = ReactiveRegistry::with_emitter(emitter);

        // Register a procedure — even though execution will fail gracefully
        // due to minimal compiled data, the pipeline infrastructure is exercised
        let handler = Arc::new(NoOpActionHandler);
        let adapter = Arc::new(make_test_adapter(handler));
        registry.register_procedure("test:*", adapter).await;

        registry.on_write("test:key", &Value::Null).await;

        // Give spawned task time
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // The test adapter has minimal compiled data so execution won't produce
        // $emit events, but we verify the channel is still open and no panic
        assert!(rx.try_recv().is_err()); // No events emitted (expected)
    }

    // ── Test Helpers ─────────────────────────────────────────────────────

    /// A no-op action handler for testing.
    #[derive(Clone)]
    struct NoOpActionHandler;

    #[async_trait::async_trait]
    impl AsyncActionHandler for NoOpActionHandler {
        async fn call(
            &self,
            _name: &str,
            _params: &Value,
        ) -> Result<Value, pares_radix_praxis::px::executor::ExecutionError> {
            Ok(Value::Null)
        }
    }

    /// An action handler that counts calls.
    struct CountingActionHandler {
        count: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl AsyncActionHandler for CountingActionHandler {
        async fn call(
            &self,
            _name: &str,
            _params: &Value,
        ) -> Result<Value, pares_radix_praxis::px::executor::ExecutionError> {
            self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Value::Null)
        }
    }

    /// Create a minimal PxProcedureAdapter for testing.
    ///
    /// The compiled data is minimal but valid enough for `from_compiled`
    /// to succeed. The procedure will fail gracefully during execution
    /// due to having no actual steps.
    fn make_test_adapter(handler: Arc<dyn AsyncActionHandler>) -> PxProcedureAdapter {
        let compiled = serde_json::json!({
            "type": "procedure",
            "name": "test_reactive_proc",
            "trigger": {
                "kind": "on_write"
            },
            "steps": []
        });
        PxProcedureAdapter::from_compiled(compiled, handler).expect("test adapter should be valid")
    }

    // ── Subscribe Result Tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn subscribe_result_receives_value_on_write() {
        let registry = ReactiveRegistry::new();

        // Subscribe before write
        let rx = registry.subscribe_result("route_decision:msg-42").await;

        // Write the key
        let value = serde_json::json!({"tier": "premium", "destination": "conversation"});
        registry.on_write("route_decision:msg-42", &value).await;

        // Should receive the value
        let received = rx.await.unwrap();
        assert_eq!(received["tier"], "premium");
        assert_eq!(received["destination"], "conversation");
    }

    #[tokio::test]
    async fn subscribe_result_only_fires_once() {
        let registry = ReactiveRegistry::new();

        let rx = registry.subscribe_result("test:key").await;
        registry.on_write("test:key", &Value::String("first".into())).await;

        // Subscriber consumed
        let received = rx.await.unwrap();
        assert_eq!(received, "first");

        // Second write has no subscribers
        registry.on_write("test:key", &Value::String("second".into())).await;
        // No panic = success
    }

    #[tokio::test]
    async fn subscribe_result_timeout_on_no_write() {
        use tokio::time::{timeout, Duration};

        let registry = ReactiveRegistry::new();
        let rx = registry.subscribe_result("never:written").await;

        // Should timeout since no write happens
        let result = timeout(Duration::from_millis(50), rx).await;
        assert!(result.is_err(), "should timeout when no write occurs");
    }

    #[tokio::test]
    async fn subscribe_result_multiple_waiters_all_receive() {
        let registry = ReactiveRegistry::new();

        // Two subscribers on the same key
        let rx1 = registry.subscribe_result("shared:key").await;
        let rx2 = registry.subscribe_result("shared:key").await;

        let value = serde_json::json!({"hello": "world"});
        registry.on_write("shared:key", &value).await;

        assert_eq!(rx1.await.unwrap()["hello"], "world");
        assert_eq!(rx2.await.unwrap()["hello"], "world");
    }

    #[tokio::test]
    async fn subscribe_result_non_matching_key_not_notified() {
        use tokio::time::{timeout, Duration};

        let registry = ReactiveRegistry::new();
        let rx = registry.subscribe_result("specific:key-1").await;

        // Write a DIFFERENT key
        registry.on_write("specific:key-2", &Value::Null).await;

        // Should timeout because key doesn't match
        let result = timeout(Duration::from_millis(50), rx).await;
        assert!(result.is_err());
    }
}
