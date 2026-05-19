//! Channel trait for spine-driven adapters.

use async_trait::async_trait;
use thiserror::Error;

use super::event::SpineEvent;
use super::pipeline::PipelineEmitter;

/// Errors from channel operations.
#[derive(Debug, Error)]
pub enum ChannelError {
    #[error("delivery failed: {0}")]
    DeliveryFailed(String),

    #[error("connection error: {0}")]
    ConnectionError(String),

    #[error("channel not ready: {0}")]
    NotReady(String),
}

/// Result of a delivery attempt.
#[derive(Debug, Clone)]
pub struct DeliveryResult {
    pub success: bool,
    pub platform_message_id: Option<String>,
}

/// Trait implemented by thin channel adapters.
///
/// A channel adapter does ONLY two things:
/// 1. Receive platform-specific events → emit SpineEvent::Inbound
/// 2. Handle DeliveryRequest events → send via platform API
#[async_trait]
pub trait SpineChannel: Send + Sync {
    /// Unique channel identifier (e.g. "telegram", "teams").
    fn channel_id(&self) -> &str;

    /// Start receiving messages from the platform.
    ///
    /// This should emit `SpineEvent::Inbound` via the provided emitter
    /// for each incoming message. Typically blocks or spawns a long-polling loop.
    async fn start_receiving(&self, emitter: PipelineEmitter) -> Result<(), ChannelError>;

    /// Deliver a message to the platform.
    ///
    /// Called by the delivery loop when a `DeliveryRequest` targets this channel.
    async fn deliver(&self, event: &SpineEvent) -> Result<DeliveryResult, ChannelError>;
}
