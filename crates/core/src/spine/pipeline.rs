//! Pipeline — the central event bus for the spine architecture.
//!
//! Events flow through the pipeline and are dispatched to registered
//! procedures. Each procedure can observe events and emit new ones.

use std::sync::Arc;

use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

use super::event::SpineEvent;

/// A procedure that processes spine events.
///
/// Procedures are registered with the pipeline and called for each event.
/// They can emit new events back into the pipeline via the provided emitter.
#[async_trait::async_trait]
pub trait SpineProcedure: Send + Sync {
    /// The name of this procedure (for logging/debugging).
    fn name(&self) -> &str;

    /// Which event types this procedure handles.
    /// Return None to receive ALL events.
    fn handles(&self) -> Option<Vec<&'static str>> {
        None
    }

    /// Process an event, optionally emitting new events.
    async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter);
}

/// Emitter for sending events into the pipeline.
#[derive(Clone)]
pub struct PipelineEmitter {
    /// The sending half of the pipeline event channel.
    pub tx: mpsc::Sender<SpineEvent>,
}

impl PipelineEmitter {
    /// Emit a new event into the pipeline.
    pub async fn emit(&self, event: SpineEvent) {
        if let Err(e) = self.tx.send(event).await {
            error!("pipeline emitter: failed to send event: {}", e);
        }
    }
}

/// The spine pipeline — event bus + procedure registry.
pub struct Pipeline {
    tx: mpsc::Sender<SpineEvent>,
    procedures: tokio::sync::RwLock<Vec<Arc<dyn SpineProcedure>>>,
    delivery_tx: broadcast::Sender<SpineEvent>,
    /// Optional reactive registry for .px procedure triggers on events.
    reactive: Option<Arc<super::reactive::ReactiveRegistry>>,
}

impl Pipeline {
    /// Create a new pipeline with the given event buffer capacity.
    ///
    /// Returns the pipeline and the receiver channel for the event loop.
    pub fn new(capacity: usize) -> (Arc<Self>, mpsc::Receiver<SpineEvent>) {
        let (tx, rx) = mpsc::channel(capacity);
        let (delivery_tx, _) = broadcast::channel(capacity);

        let pipeline = Arc::new(Self {
            tx,
            procedures: tokio::sync::RwLock::new(Vec::new()),
            delivery_tx,
            reactive: None,
        });

        (pipeline, rx)
    }

    /// Create a new pipeline with a reactive registry attached.
    pub fn with_reactive(
        capacity: usize,
        reactive: Arc<super::reactive::ReactiveRegistry>,
    ) -> (Arc<Self>, mpsc::Receiver<SpineEvent>) {
        let (tx, rx) = mpsc::channel(capacity);
        let (delivery_tx, _) = broadcast::channel(capacity);

        let pipeline = Arc::new(Self {
            tx,
            procedures: tokio::sync::RwLock::new(Vec::new()),
            delivery_tx,
            reactive: Some(reactive),
        });

        (pipeline, rx)
    }

    /// Register a procedure with the pipeline.
    pub async fn register(&self, procedure: Arc<dyn SpineProcedure>) {
        info!(procedure = procedure.name(), "pipeline: registered procedure");
        self.procedures.write().await.push(procedure);
    }

    /// Get an emitter for sending events into the pipeline.
    pub fn emitter(&self) -> PipelineEmitter {
        PipelineEmitter {
            tx: self.tx.clone(),
        }
    }

    /// Subscribe to delivery events (DeliveryRequest broadcast).
    pub fn subscribe_deliveries(&self) -> broadcast::Receiver<SpineEvent> {
        self.delivery_tx.subscribe()
    }

    /// Run the pipeline event loop. Processes events and dispatches to procedures.
    ///
    /// This should be spawned as a tokio task.
    pub async fn run(self: &Arc<Self>, mut rx: mpsc::Receiver<SpineEvent>) {
        info!("pipeline: event loop started");

        while let Some(event) = rx.recv().await {
            debug!(
                event_type = event.event_type(),
                event_id = event.id(),
                "pipeline: processing event"
            );

            // Broadcast delivery requests to channel adapters
            if matches!(event, SpineEvent::DeliveryRequest { .. }) {
                if let Err(e) = self.delivery_tx.send(event.clone()) {
                    warn!("pipeline: no delivery subscribers: {}", e);
                }
            }

            // Dispatch to all matching procedures
            let procedures = self.procedures.read().await;
            let emitter = self.emitter();

            for proc in procedures.iter() {
                let should_handle = match proc.handles() {
                    Some(types) => types.contains(&event.event_type()),
                    None => true,
                };

                if should_handle {
                    proc.handle(&event, &emitter).await;
                }
            }

            // Notify reactive registry — fire .px procedures on matching writes
            if let Some(ref reactive) = self.reactive {
                let key = format!("{}:{}", event.event_type(), event.id());
                let value = serde_json::json!({
                    "type": event.event_type(),
                    "id": event.id(),
                });
                reactive.on_write(&key, &value).await;
            }
        }

        info!("pipeline: event loop ended (channel closed)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoProcedure;

    #[async_trait::async_trait]
    impl SpineProcedure for EchoProcedure {
        fn name(&self) -> &str {
            "echo"
        }

        fn handles(&self) -> Option<Vec<&'static str>> {
            Some(vec!["inbound"])
        }

        async fn handle(&self, event: &SpineEvent, emitter: &PipelineEmitter) {
            if let SpineEvent::Inbound {
                chat_id, content, ..
            } = event
            {
                emitter
                    .emit(SpineEvent::DeliveryRequest {
                        id: SpineEvent::new_id(),
                        channel: "test".into(),
                        chat_id: chat_id.clone(),
                        content: format!("echo: {}", content),
                        metadata: serde_json::json!({}),
                    })
                    .await;
            }
        }
    }

    #[tokio::test]
    async fn pipeline_dispatches_to_procedures() {
        let (pipeline, rx) = Pipeline::new(16);
        pipeline.register(Arc::new(EchoProcedure)).await;

        let mut delivery_rx = pipeline.subscribe_deliveries();
        let emitter = pipeline.emitter();

        // Start pipeline loop
        let pipeline_clone = Arc::clone(&pipeline);
        let handle = tokio::spawn(async move {
            pipeline_clone.run(rx).await;
        });

        // Emit an inbound event
        emitter
            .emit(SpineEvent::Inbound {
                id: SpineEvent::new_id(),
                source: "test".into(),
                chat_id: "123".into(),
                sender: "user".into(),
                content: "hello".into(),
                metadata: serde_json::json!({}),
            })
            .await;

        // Should receive a delivery request from the echo procedure
        let delivered = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            delivery_rx.recv(),
        )
        .await
        .expect("timeout")
        .expect("recv error");

        if let SpineEvent::DeliveryRequest { content, .. } = delivered {
            assert_eq!(content, "echo: hello");
        } else {
            panic!("expected DeliveryRequest, got {:?}", delivered);
        }

        // Cleanup — abort the pipeline loop task
        handle.abort();
    }

    #[tokio::test]
    async fn procedure_filter_skips_non_matching_events() {
        let (pipeline, rx) = Pipeline::new(16);
        pipeline.register(Arc::new(EchoProcedure)).await;

        let mut delivery_rx = pipeline.subscribe_deliveries();
        let emitter = pipeline.emitter();

        let pipeline_clone = Arc::clone(&pipeline);
        let handle = tokio::spawn(async move {
            pipeline_clone.run(rx).await;
        });

        // Emit a delivery request (not "inbound") — EchoProcedure should not fire
        emitter
            .emit(SpineEvent::DeliveryRequest {
                id: SpineEvent::new_id(),
                channel: "test".into(),
                chat_id: "123".into(),
                content: "passthrough".into(),
                metadata: serde_json::json!({}),
            })
            .await;

        // The delivery_rx should get the event (broadcast), but no new echo event
        let delivered = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            delivery_rx.recv(),
        )
        .await
        .expect("timeout")
        .expect("recv error");

        if let SpineEvent::DeliveryRequest { content, .. } = delivered {
            assert_eq!(content, "passthrough"); // Original, not "echo: ..."
        } else {
            panic!("unexpected event");
        }

        // No second event should arrive
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            delivery_rx.recv(),
        )
        .await;
        assert!(result.is_err(), "should timeout — no extra events");

        handle.abort();
    }
}
