//! Thin Telegram channel adapter for the spine pipeline.
//!
//! This replaces the fat Telegram adapter. It does ONLY:
//! 1. Receive Telegram updates → emit SpineEvent::Inbound
//! 2. Subscribe to DeliveryRequest events → send via Telegram Bot API
//!
//! All logic (model calls, slash commands, cerebellum, history, tools)
//! lives in spine procedures. This adapter is interchangeable.

use async_trait::async_trait;
use teloxide::prelude::*;
use teloxide::types::ChatId;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use pares_agens_core::spine::channel::{ChannelError, DeliveryResult, SpineChannel};
use pares_agens_core::spine::event::SpineEvent;
use pares_agens_core::spine::pipeline::PipelineEmitter;

/// Configuration for the thin Telegram spine channel.
pub struct TelegramSpineConfig {
    /// Bot token from BotFather.
    pub token: String,
}

/// Thin Telegram channel — input/output only.
pub struct TelegramSpineChannel {
    config: TelegramSpineConfig,
}

impl TelegramSpineChannel {
    /// Create a new thin Telegram channel.
    pub fn new(config: TelegramSpineConfig) -> Self {
        Self { config }
    }

    /// Run the delivery loop — subscribes to pipeline delivery events
    /// and sends them via Telegram.
    pub async fn run_delivery_loop(&self, mut delivery_rx: broadcast::Receiver<SpineEvent>) {
        let bot = Bot::new(&self.config.token);
        info!("telegram_spine: delivery loop started");

        loop {
            match delivery_rx.recv().await {
                Ok(SpineEvent::DeliveryRequest {
                    id,
                    channel,
                    chat_id,
                    content,
                    ..
                }) => {
                    if channel != "telegram" {
                        continue;
                    }

                    let chat_id_parsed: i64 = match chat_id.parse() {
                        Ok(id) => id,
                        Err(_) => {
                            warn!(event_id = %id, chat_id = %chat_id, "telegram_spine: invalid chat_id");
                            continue;
                        }
                    };

                    debug!(event_id = %id, chat_id = chat_id_parsed, "telegram_spine: delivering");

                    match bot.send_message(ChatId(chat_id_parsed), &content).await {
                        Ok(msg) => {
                            debug!(message_id = msg.id.0, "telegram_spine: delivered");
                        }
                        Err(e) => {
                            error!(error = %e, "telegram_spine: delivery failed");
                        }
                    }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "telegram_spine: delivery receiver lagged");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("telegram_spine: pipeline closed, stopping");
                    break;
                }
            }
        }
    }
}

#[async_trait]
impl SpineChannel for TelegramSpineChannel {
    fn channel_id(&self) -> &str {
        "telegram"
    }

    async fn start_receiving(&self, emitter: PipelineEmitter) -> Result<(), ChannelError> {
        let bot = Bot::new(&self.config.token);

        // Validate token upfront — teloxide's dispatch() panics on invalid tokens
        // (teloxide-0.17 dispatcher.rs:385 unwraps get_me). Catch it here cleanly.
        match bot.get_me().await {
            Ok(me) => {
                info!(
                    bot_username = %me.username(),
                    "telegram_spine: token validated, starting receiver"
                );
            }
            Err(e) => {
                return Err(ChannelError::ConnectionError(format!(
                    "Telegram token validation failed: {e}. Check your TELEGRAM_TOKEN."
                )));
            }
        }

        let handler = Update::filter_message().endpoint(move |msg: Message| {
            let emitter = emitter.clone();
            async move {
                let text = msg.text().unwrap_or("").to_string();
                if text.is_empty() {
                    return Ok::<(), teloxide::RequestError>(());
                }

                let sender = msg
                    .from
                    .as_ref()
                    .map(|u| u.username.clone().unwrap_or_else(|| u.first_name.clone()))
                    .unwrap_or_else(|| "unknown".to_string());

                emitter
                    .emit(SpineEvent::Inbound {
                        id: SpineEvent::new_id(),
                        source: "telegram".into(),
                        chat_id: msg.chat.id.0.to_string(),
                        sender,
                        content: text,
                        metadata: serde_json::json!({
                            "message_id": msg.id.0,
                        }),
                    })
                    .await;

                Ok(())
            }
        });

        teloxide::dispatching::Dispatcher::builder(bot, handler)
            .build()
            .dispatch()
            .await;

        Ok(())
    }

    async fn deliver(&self, event: &SpineEvent) -> Result<DeliveryResult, ChannelError> {
        let SpineEvent::DeliveryRequest {
            chat_id, content, ..
        } = event
        else {
            return Ok(DeliveryResult {
                success: false,
                platform_message_id: None,
            });
        };

        let chat_id_parsed: i64 = chat_id
            .parse()
            .map_err(|_| ChannelError::DeliveryFailed("invalid chat_id".into()))?;

        let bot = Bot::new(&self.config.token);
        let msg = bot
            .send_message(ChatId(chat_id_parsed), content)
            .await
            .map_err(|e| ChannelError::DeliveryFailed(e.to_string()))?;

        Ok(DeliveryResult {
            success: true,
            platform_message_id: Some(msg.id.0.to_string()),
        })
    }
}
