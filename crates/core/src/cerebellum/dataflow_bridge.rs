//! Dataflow bridge: connects the cerebellum to the queue-driven procedure graph.
//!
//! Instead of trigger-matching (old model), the cerebellum pushes event data
//! into the graph's input queues and reads results from output queues.
//!
//! The graph is built at startup from .px files with typed signatures.
//! At runtime, data flows through queues — no scheduling decisions needed.

use pares_radix_praxis::dataflow::{
    AsyncDataflowGraph, DataflowConfig, Datum, ProcedureNode,
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
}

impl DataflowBridge {
    /// Create a new bridge with default config.
    pub fn new() -> Self {
        Self {
            graph: AsyncDataflowGraph::new(),
            active: false,
        }
    }

    /// Create with custom depth/queue limits.
    pub fn with_config(config: DataflowConfig) -> Self {
        Self {
            graph: AsyncDataflowGraph::with_config(config),
            active: false,
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
    /// This is the main entry point for the cerebellum:
    /// ```text
    /// event arrives → push to "inbound" queue → graph runs → read from "response" queue
    /// ```
    pub async fn process_event(
        &self,
        event_type: &str,
        content: &str,
        context: &str,
        handler: Arc<dyn pares_radix_praxis::dataflow::AsyncActionHandler>,
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
            .run_to_completion(handler)
            .await
            .map_err(|e| format!("execution failed: {e}"))?;

        info!(procedures_fired = fired, "dataflow graph quiescent");

        // Read from the response output queue
        // TODO: implement output reading once AsyncDataflowGraph exposes peek/pop
        Ok(None)
    }
}

impl Default for DataflowBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// Action handler for the cerebellum's dataflow graph.
///
/// Maps action names (from .px steps) to real implementations.
/// This is the effect boundary: pure procedures call actions like
/// `classify`, `model_complete`, `detect_intent` etc.
///
/// For now, this is a stub that passes through inputs as outputs.
/// The real implementations will delegate to the model client, tools, etc.
pub struct CerebellumActionHandler;

#[async_trait::async_trait]
impl pares_radix_praxis::dataflow::AsyncActionHandler for CerebellumActionHandler {
    async fn call(
        &self,
        name: &str,
        params: &Value,
    ) -> Result<Value, pares_radix_praxis::dataflow::ExecutionError> {
        // Stub: echo the action and args back as output.
        // Real implementation will route to model/tools/memory.
        Ok(json!({
            "action": name,
            "result": params,
            "status": "stub"
        }))
    }
}
