//! Dataflow bridge: connects the cerebellum to the queue-driven procedure graph.
//!
//! Instead of trigger-matching (old model), the cerebellum pushes event data
//! into the graph's input queues and reads results from output queues.
//!
//! The graph is built at startup from .px files with typed signatures.
//! At runtime, data flows through queues — no scheduling decisions needed.

use pares_radix_praxis::dataflow::{
    AsyncDataflowGraph, DataflowConfig, Datum, ProcedureNode, AsyncActionHandler,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

/// The dataflow-driven replacement for PxBridge.
///
/// Procedures are pre-wired at startup. Events push data into input queues;
/// results materialize in output queues.
pub struct DataflowBridge {
    graph: AsyncDataflowGraph,
    /// Whether the graph has any procedures loaded.
    active: bool,
    /// Action handler for side-effect dispatch.
    handler: Arc<dyn AsyncActionHandler>,
}

impl DataflowBridge {
    /// Create a new bridge with default config and an action handler.
    pub fn new(handler: Arc<dyn AsyncActionHandler>) -> Self {
        Self {
            graph: AsyncDataflowGraph::new(),
            active: false,
            handler,
        }
    }

    /// Create with custom depth/queue limits.
    pub fn with_config(config: DataflowConfig, handler: Arc<dyn AsyncActionHandler>) -> Self {
        Self {
            graph: AsyncDataflowGraph::with_config(config),
            active: false,
            handler,
        }
    }

    /// Load a procedure node into the graph.
    pub async fn register(&mut self, node: ProcedureNode) -> Result<(), String> {
        self.graph
            .register(node)
            .await
            .map_err(|e| format!("{e}"))?;
        self.active = true;
        Ok(())
    }

    /// Whether any procedures are loaded.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Push an inbound event into the graph and run to quiescence.
    /// Returns the final output value from the designated result queue.
    ///
    /// Output queue priority: "route" > "classification" > "model_response"
    /// (whichever has data first wins)
    pub async fn process_event(
        &self,
        event_type: &str,
        content: &str,
        context: &str,
    ) -> Result<Option<Value>, String> {
        // Package event data as a datum
        let event_datum = Datum::root(json!({
            "type": event_type,
            "content": content,
            "context": context,
        }));

        // Push to the inbound queue
        self.graph
            .push("inbound", event_datum)
            .await
            .map_err(|e| format!("push failed: {e}"))?;

        // Run the graph to quiescence — all ready procedures fire concurrently
        let fired = self
            .graph
            .run_to_completion(self.handler.clone())
            .await
            .map_err(|e| format!("execution failed: {e}"))?;

        info!(procedures_fired = fired, "dataflow graph quiescent");

        if fired == 0 {
            return Ok(None);
        }

        // Read from output queues in priority order
        for queue_name in &["route", "classification", "model_response"] {
            if let Some(datum) = self.graph.pop(queue_name).await {
                return Ok(Some(datum.value));
            }
        }

        // Procedures fired but no output in known queues
        Ok(None)
    }
}

/// Re-exports the existing CerebellumActionHandler so the dataflow bridge
/// can use the same IO boundary as the trigger-based px_bridge.
pub use super::actions::CerebellumActionHandler;

/// Adapter that wraps our local AsyncActionHandler to satisfy the pluresdb-px trait.
/// Both traits have `async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError>`
/// but they're from different crates, so we need a thin wrapper.
pub struct DataflowActionAdapter {
    inner: Arc<dyn crate::px_adapter::AsyncActionHandler>,
}

impl DataflowActionAdapter {
    pub fn new(handler: Arc<dyn crate::px_adapter::AsyncActionHandler>) -> Self {
        Self { inner: handler }
    }
}

#[async_trait::async_trait]
impl pares_radix_praxis::dataflow::AsyncActionHandler for DataflowActionAdapter {
    async fn call(
        &self,
        name: &str,
        params: &Value,
    ) -> Result<Value, pares_radix_praxis::dataflow::ExecutionError> {
        // Both traits use the same ExecutionError type (pluresdb_px::px::executor::ExecutionError)
        // re-exported through different paths. Direct pass-through works.
        self.inner.call(name, params).await
    }
}
