//! In-process HTTP channel integration test — Canonical Verify Stage (C-TEST-001)
//!
//! Tests the HTTP spine channel WITHOUT spawning a separate binary.
//! Starts the axum server in-process, mocks the pipeline with an echo loop,
//! and verifies HTTP API behavior.
//!
//! This proves pares-radix works as a platform through its HTTP API —
//! no Telegram, no external dependencies.
//!
//! Run: cargo test -p pares-agens-channels --test http_channel_verify

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::sync::{broadcast, mpsc};

use pares_agens_channels::http_spine::{
    start_http_server, HttpSpineChannel, HttpSpineConfig, PendingResponses,
};
use pares_agens_core::spine::event::SpineEvent;
use pares_agens_core::spine::pipeline::PipelineEmitter;

/// Start an echo server: inbound messages get echoed back through the delivery path.
/// Returns (port, background tasks handle, event receiver for inspection).
async fn start_echo_server(timeout_secs: u64) -> (u16, mpsc::Receiver<SpineEvent>) {
    // Find a free port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener); // Free the port for the HTTP server

    // Pipeline event bus (mpsc for inbound, broadcast for delivery)
    let (inbound_tx, mut inbound_rx) = mpsc::channel::<SpineEvent>(64);
    let (delivery_tx, _) = broadcast::channel::<SpineEvent>(64);

    let emitter = PipelineEmitter { tx: inbound_tx };
    let pending = Arc::new(PendingResponses::default());

    let config = HttpSpineConfig {
        port,
        bearer_token: None,
        timeout_seconds: timeout_secs,
    };

    // Start delivery loop (routes broadcast delivery events to pending HTTP requests)
    let channel = HttpSpineChannel::new(config.clone());
    let delivery_rx = delivery_tx.subscribe();
    let pending_for_delivery = Arc::clone(&pending);
    tokio::spawn(async move {
        channel
            .run_delivery_loop(delivery_rx, pending_for_delivery)
            .await;
    });

    // Echo loop: reads from inbound_rx, echoes via delivery_tx
    let delivery_tx_clone = delivery_tx.clone();
    let (inspect_tx, inspect_rx) = mpsc::channel::<SpineEvent>(64);
    tokio::spawn(async move {
        while let Some(event) = inbound_rx.recv().await {
            // Send to inspector
            let _ = inspect_tx.send(event.clone()).await;

            if let SpineEvent::Inbound {
                ref id,
                ref chat_id,
                ref content,
                ..
            } = event
            {
                let delivery = SpineEvent::DeliveryRequest {
                    id: format!("echo-{id}"),
                    channel: "http".into(),
                    chat_id: chat_id.clone(),
                    content: format!("Echo: {content}"),
                    metadata: json!({}),
                };
                let _ = delivery_tx_clone.send(delivery);
            }
        }
    });

    // Start HTTP server
    let pending_for_server = Arc::clone(&pending);
    tokio::spawn(async move {
        let _ = start_http_server(emitter, pending_for_server, config).await;
    });

    // Wait for server to bind
    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if tokio::time::Instant::now() > deadline {
            panic!("Echo server failed to start on port {port}");
        }
        match client
            .get(format!("http://127.0.0.1:{port}/v1/health"))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => break,
            _ => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }

    (port, inspect_rx)
}

// ── Health Tests ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_ok() {
    let (port, _rx) = start_echo_server(5).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{port}/v1/health"))
        .await
        .expect("health request failed");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["channel"], "http");
    assert!(body["version"].is_string());
    assert!(!body["version"].as_str().unwrap().is_empty());
}

// ── Chat Validation Tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn chat_rejects_empty_message() {
    let (port, _rx) = start_echo_server(5).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{port}/v1/chat"))
        .json(&json!({ "message": "" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn chat_rejects_whitespace_only() {
    let (port, _rx) = start_echo_server(5).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{port}/v1/chat"))
        .json(&json!({ "message": "   \n\t  " }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

// ── Chat Echo Tests (prove full pipeline works) ───────────────────────────────

#[tokio::test]
async fn chat_echoes_message() {
    let (port, _rx) = start_echo_server(5).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:{port}/v1/chat"))
        .json(&json!({
            "message": "hello world",
            "sender": "test-harness"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["response"], "Echo: hello world");
    assert!(body["id"].is_string());
    assert!(!body["id"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn chat_uses_sender_for_default_session() {
    let (port, mut rx) = start_echo_server(5).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:{port}/v1/chat"))
        .json(&json!({
            "message": "identify me",
            "sender": "alice"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    // Check that the inbound event used sender-based session
    if let Ok(event) = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
        if let Some(SpineEvent::Inbound { chat_id, sender, .. }) = event {
            assert_eq!(sender, "alice");
            assert_eq!(chat_id, "http:alice"); // Default session derivation
        }
    }
}

#[tokio::test]
async fn chat_explicit_session_id() {
    let (port, mut rx) = start_echo_server(5).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:{port}/v1/chat"))
        .json(&json!({
            "message": "custom session",
            "sender": "bob",
            "session_id": "my-custom-session"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    // Verify the custom session_id was used
    if let Ok(event) = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
        if let Some(SpineEvent::Inbound { chat_id, .. }) = event {
            assert_eq!(chat_id, "my-custom-session");
        }
    }
}

// ── Session Isolation Tests ───────────────────────────────────────────────────

#[tokio::test]
async fn sessions_are_independent() {
    let (port, _rx) = start_echo_server(5).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    // Two different sessions should both work
    let resp_a = client
        .post(format!("http://127.0.0.1:{port}/v1/chat"))
        .json(&json!({ "message": "A says hi", "session_id": "session-a" }))
        .send()
        .await
        .unwrap();

    let resp_b = client
        .post(format!("http://127.0.0.1:{port}/v1/chat"))
        .json(&json!({ "message": "B says hi", "session_id": "session-b" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp_a.status(), 200);
    assert_eq!(resp_b.status(), 200);

    let body_a: serde_json::Value = resp_a.json().await.unwrap();
    let body_b: serde_json::Value = resp_b.json().await.unwrap();

    assert_eq!(body_a["response"], "Echo: A says hi");
    assert_eq!(body_b["response"], "Echo: B says hi");
}

// ── Timeout Tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn chat_returns_504_on_timeout() {
    // Start server with no echo loop — just the HTTP server + delivery loop
    // but nothing will produce delivery events → timeout
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let (inbound_tx, _inbound_rx) = mpsc::channel::<SpineEvent>(64);
    let (delivery_tx, _) = broadcast::channel::<SpineEvent>(64);

    let emitter = PipelineEmitter { tx: inbound_tx };
    let pending = Arc::new(PendingResponses::default());

    let config = HttpSpineConfig {
        port,
        bearer_token: None,
        timeout_seconds: 1, // 1 second timeout for fast test
    };

    // Delivery loop (but nothing will emit deliveries)
    let channel = HttpSpineChannel::new(config.clone());
    let delivery_rx = delivery_tx.subscribe();
    let pending_for_delivery = Arc::clone(&pending);
    tokio::spawn(async move {
        channel
            .run_delivery_loop(delivery_rx, pending_for_delivery)
            .await;
    });

    let pending_for_server = Arc::clone(&pending);
    tokio::spawn(async move {
        let _ = start_http_server(emitter, pending_for_server, config).await;
    });

    // Wait for bind
    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:{port}/v1/chat"))
        .json(&json!({ "message": "will timeout", "sender": "test" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 504);
}
