//! Integration test: streaming broadcast bridge.
//!
//! Verifies that when the Serve path's event handler calls `handle_event_streaming`,
//! the mpsc→broadcast bridge correctly forwards StreamDelta tokens to any broadcast
//! subscriber (simulating TelegramAdapter's progressive message editing).

use tokio::sync::{broadcast, mpsc};

use pares_agens_core::model::StreamDelta;

/// Simulates the exact bridging logic used in `run_adapter_with_recovery`:
/// - Agent writes StreamDelta tokens to an mpsc sender
/// - Bridge task forwards them to a broadcast sender
/// - Adapter (subscriber) receives them from the broadcast receiver
#[tokio::test]
async fn streaming_bridge_forwards_deltas_to_broadcast() {
    // Create the broadcast channel (what TelegramAdapter subscribes to)
    let (broadcast_tx, mut broadcast_rx) = broadcast::channel::<StreamDelta>(256);

    // Create the mpsc channel (what Agent::handle_event_streaming writes to)
    let (mpsc_tx, mut mpsc_rx) = mpsc::unbounded_channel::<StreamDelta>();

    // Bridge task: mpsc → broadcast (matches the code in main.rs)
    let broadcast_tx_for_bridge = Some(broadcast_tx.clone());
    tokio::spawn(async move {
        while let Some(delta) = mpsc_rx.recv().await {
            if let Some(ref btx) = broadcast_tx_for_bridge {
                let _ = btx.send(delta);
            }
        }
    });

    // Simulate agent producing streaming tokens
    mpsc_tx.send(StreamDelta::Content("Hello".into())).unwrap();
    mpsc_tx.send(StreamDelta::Content(", world".into())).unwrap();
    mpsc_tx.send(StreamDelta::Content("!".into())).unwrap();
    mpsc_tx.send(StreamDelta::Done).unwrap();
    drop(mpsc_tx); // Close the channel

    // Verify all deltas arrive on the broadcast side
    let mut received = Vec::new();
    loop {
        match broadcast_rx.recv().await {
            Ok(delta) => {
                let is_done = matches!(&delta, StreamDelta::Done);
                received.push(delta);
                if is_done {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
        }
    }

    assert_eq!(received.len(), 4, "expected 4 deltas (3 content + done)");
    assert!(matches!(&received[0], StreamDelta::Content(s) if s == "Hello"));
    assert!(matches!(&received[1], StreamDelta::Content(s) if s == ", world"));
    assert!(matches!(&received[2], StreamDelta::Content(s) if s == "!"));
    assert!(matches!(&received[3], StreamDelta::Done));
}

/// Verifies that when stream_tx is None (no broadcast configured), the bridge
/// doesn't panic — it just drains silently.
#[tokio::test]
async fn streaming_bridge_handles_none_broadcast_gracefully() {
    let (mpsc_tx, mut mpsc_rx) = mpsc::unbounded_channel::<StreamDelta>();

    // Bridge with None broadcast (old behavior / misconfigured)
    let broadcast_tx_for_bridge: Option<broadcast::Sender<StreamDelta>> = None;
    tokio::spawn(async move {
        while let Some(delta) = mpsc_rx.recv().await {
            if let Some(ref btx) = broadcast_tx_for_bridge {
                let _ = btx.send(delta);
            }
        }
    });

    // Should not panic
    mpsc_tx.send(StreamDelta::Content("test".into())).unwrap();
    mpsc_tx.send(StreamDelta::Done).unwrap();
    drop(mpsc_tx);

    // Give the bridge time to drain
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    // If we got here without panic, the test passes
}

/// Verifies that tool call deltas also bridge correctly (not just Content).
#[tokio::test]
async fn streaming_bridge_forwards_tool_call_deltas() {
    let (broadcast_tx, mut broadcast_rx) = broadcast::channel::<StreamDelta>(256);
    let (mpsc_tx, mut mpsc_rx) = mpsc::unbounded_channel::<StreamDelta>();

    let broadcast_tx_for_bridge = Some(broadcast_tx.clone());
    tokio::spawn(async move {
        while let Some(delta) = mpsc_rx.recv().await {
            if let Some(ref btx) = broadcast_tx_for_bridge {
                let _ = btx.send(delta);
            }
        }
    });

    // Simulate a tool call stream
    mpsc_tx
        .send(StreamDelta::ToolCallStart {
            index: 0,
            id: "call_abc".into(),
            name: "run_command".into(),
        })
        .unwrap();
    mpsc_tx
        .send(StreamDelta::ToolCallDelta {
            index: 0,
            arguments: r#"{"cmd":"ls"}"#.into(),
        })
        .unwrap();
    mpsc_tx.send(StreamDelta::Done).unwrap();
    drop(mpsc_tx);

    let mut received = Vec::new();
    loop {
        match broadcast_rx.recv().await {
            Ok(delta) => {
                let is_done = matches!(&delta, StreamDelta::Done);
                received.push(delta);
                if is_done {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
        }
    }

    assert_eq!(received.len(), 3);
    assert!(matches!(&received[0], StreamDelta::ToolCallStart { name, .. } if name == "run_command"));
    assert!(matches!(&received[1], StreamDelta::ToolCallDelta { arguments, .. } if arguments.contains("ls")));
    assert!(matches!(&received[2], StreamDelta::Done));
}

/// Simulates multiple subscribers (e.g., if both Telegram progressive editing
/// and a logging subscriber are attached to the same broadcast).
#[tokio::test]
async fn streaming_bridge_supports_multiple_subscribers() {
    let (broadcast_tx, mut rx1) = broadcast::channel::<StreamDelta>(256);
    let mut rx2 = broadcast_tx.subscribe();

    let (mpsc_tx, mut mpsc_rx) = mpsc::unbounded_channel::<StreamDelta>();

    let broadcast_tx_for_bridge = Some(broadcast_tx.clone());
    tokio::spawn(async move {
        while let Some(delta) = mpsc_rx.recv().await {
            if let Some(ref btx) = broadcast_tx_for_bridge {
                let _ = btx.send(delta);
            }
        }
    });

    mpsc_tx.send(StreamDelta::Content("shared".into())).unwrap();
    mpsc_tx.send(StreamDelta::Done).unwrap();
    drop(mpsc_tx);

    // Both subscribers should receive the same deltas
    let d1 = rx1.recv().await.unwrap();
    let d2 = rx2.recv().await.unwrap();
    assert!(matches!(&d1, StreamDelta::Content(s) if s == "shared"));
    assert!(matches!(&d2, StreamDelta::Content(s) if s == "shared"));
}
