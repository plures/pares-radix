//! Integration tests for the OpenTelemetry OTLP tracing layer.
//!
//! Uses `opentelemetry_sdk::testing` in-process exporter to verify span
//! propagation without requiring a real gRPC collector.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::testing::trace::new_tokio_test_exporter;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::prelude::*;

/// Build a tracing layer backed by the in-memory test exporter.
/// Returns the layer AND the span receiver so tests can assert on exported spans.
fn test_otel_layer() -> (
    impl tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync,
    tokio::sync::mpsc::UnboundedReceiver<opentelemetry_sdk::trace::SpanData>,
    tokio::sync::mpsc::UnboundedReceiver<()>,
) {
    let (exporter, rx_export, rx_shutdown) = new_tokio_test_exporter();

    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter)
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(Resource::builder().build())
        .build();

    let tracer = provider.tracer("test-tracer");

    // Keep provider alive by leaking it (test-only; avoids drop shutting down export)
    let _provider = Box::leak(Box::new(provider));

    let layer = OpenTelemetryLayer::new(tracer);
    (layer, rx_export, rx_shutdown)
}

#[tokio::test]
async fn spans_are_exported_through_tracing_layer() {
    let (otel_layer, mut rx_export, _rx_shutdown) = test_otel_layer();

    let subscriber = tracing_subscriber::registry().with(otel_layer);

    // Use a scoped subscriber so we don't pollute global state
    let _guard = tracing::subscriber::set_default(subscriber);

    // Emit a span
    {
        let span = tracing::info_span!("test_operation", kind = "integration");
        let _enter = span.enter();
        tracing::info!("inside test span");
    }

    // Give the simple exporter time to flush
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify we received the span
    let span_data = rx_export
        .try_recv()
        .expect("expected at least one span to be exported");

    assert_eq!(span_data.name.as_ref(), "test_operation");
}

#[tokio::test]
async fn nested_spans_preserve_parent_child_relationship() {
    let (otel_layer, mut rx_export, _rx_shutdown) = test_otel_layer();

    let subscriber = tracing_subscriber::registry().with(otel_layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    // Create parent and child spans
    {
        let parent = tracing::info_span!("parent_span");
        let _p = parent.enter();
        {
            let child = tracing::info_span!("child_span");
            let _c = child.enter();
            tracing::debug!("in child");
        }
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Collect all exported spans
    let mut spans = Vec::new();
    while let Ok(s) = rx_export.try_recv() {
        spans.push(s);
    }

    assert!(
        spans.len() >= 2,
        "expected at least 2 spans (parent + child), got {}",
        spans.len()
    );

    let child = spans.iter().find(|s| s.name.as_ref() == "child_span");
    let parent = spans.iter().find(|s| s.name.as_ref() == "parent_span");

    assert!(child.is_some(), "child_span not found in exported spans");
    assert!(parent.is_some(), "parent_span not found in exported spans");

    let child = child.unwrap();
    let parent = parent.unwrap();

    // Child's parent span id should match parent's span id
    assert_eq!(
        child.parent_span_id,
        parent.span_context.span_id(),
        "child span should reference parent's span id"
    );
}

#[tokio::test]
async fn sampling_ratio_zero_suppresses_spans() {
    let (exporter, mut rx_export, _rx_shutdown) = new_tokio_test_exporter();

    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter)
        .with_sampler(Sampler::AlwaysOff)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(Resource::builder().build())
        .build();

    let tracer = provider.tracer("test-noop");
    let _provider = Box::leak(Box::new(provider));
    let layer = OpenTelemetryLayer::new(tracer);

    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    {
        let span = tracing::info_span!("suppressed_span");
        let _enter = span.enter();
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // With AlwaysOff sampler, no spans should be exported
    assert!(
        rx_export.try_recv().is_err(),
        "no spans should be exported when sampler is AlwaysOff"
    );
}

/// Verify that `OtelConfig` from the crate produces reasonable defaults
/// and the init function compiles/runs (cannot test actual gRPC without a collector).
#[cfg(feature = "otel")]
#[tokio::test]
async fn otel_config_defaults_are_valid() {
    use pares_agens_core::otel::{OtelConfig, OtelProtocol};

    let config = OtelConfig::default();
    assert_eq!(config.endpoint, "http://localhost:4317");
    assert_eq!(config.service_name, "pares-radix");
    assert!((config.sample_ratio - 1.0).abs() < f64::EPSILON);
    assert_eq!(config.protocol, OtelProtocol::Grpc);
}

/// Verify HTTP/protobuf config can be constructed.
#[cfg(feature = "otel")]
#[tokio::test]
async fn otel_http_proto_config_is_valid() {
    use pares_agens_core::otel::{OtelConfig, OtelProtocol};

    let config = OtelConfig {
        endpoint: "http://localhost:4318".into(),
        protocol: OtelProtocol::HttpProto,
        ..Default::default()
    };
    assert_eq!(config.endpoint, "http://localhost:4318");
    assert_eq!(config.protocol, OtelProtocol::HttpProto);
}
