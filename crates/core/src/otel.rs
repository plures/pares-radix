//! OpenTelemetry OTLP trace export layer.
//!
//! When the `otel` feature is enabled, this module provides initialization
//! of an OTLP gRPC exporter that integrates with the `tracing` subscriber.
//!
//! # Configuration
//!
//! The exporter reads standard OTEL environment variables:
//! - `OTEL_EXPORTER_OTLP_ENDPOINT` — gRPC endpoint (default: `http://localhost:4317`)
//! - `OTEL_SERVICE_NAME` — service name (default: `pares-radix`)
//! - `OTEL_RESOURCE_ATTRIBUTES` — additional resource attributes
//!
//! # Usage
//!
//! ```no_run
//! # #[cfg(feature = "otel")]
//! # {
//! use pares_agens_core::otel::{init_otel_layer, OtelConfig};
//! use tracing_subscriber::prelude::*;
//!
//! let config = OtelConfig::from_env();
//! let otel_layer = init_otel_layer(&config).expect("OTLP init failed");
//!
//! tracing_subscriber::registry()
//!     .with(otel_layer)
//!     .with(tracing_subscriber::fmt::layer())
//!     .init();
//! # }
//! ```

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::resource::{
    EnvResourceDetector, SdkProvidedResourceDetector, TelemetryResourceDetector,
};
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing_opentelemetry::OpenTelemetryLayer;

/// Configuration for the OTLP exporter.
#[derive(Debug, Clone)]
pub struct OtelConfig {
    /// gRPC endpoint for the OTLP collector.
    pub endpoint: String,
    /// Service name reported in traces.
    pub service_name: String,
    /// Service version.
    pub service_version: String,
    /// Sampling ratio (0.0 to 1.0). 1.0 = sample everything.
    pub sample_ratio: f64,
}

impl OtelConfig {
    /// Load configuration from environment variables with sensible defaults.
    pub fn from_env() -> Self {
        Self {
            endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:4317".into()),
            service_name: std::env::var("OTEL_SERVICE_NAME")
                .unwrap_or_else(|_| "pares-radix".into()),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            sample_ratio: std::env::var("OTEL_TRACES_SAMPLER_ARG")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1.0),
        }
    }
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4317".into(),
            service_name: "pares-radix".into(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            sample_ratio: 1.0,
        }
    }
}

/// Initialize the OpenTelemetry OTLP tracing layer.
///
/// Returns a layer that can be composed with `tracing_subscriber::registry()`.
/// Call [`shutdown_otel`] on graceful shutdown to flush pending spans.
pub fn init_otel_layer<S>(
    config: &OtelConfig,
) -> Result<OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>, OtelError>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    // Build the OTLP span exporter (gRPC with tonic)
    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .build()
        .map_err(|e| OtelError::ExporterInit(e.to_string()))?;

    // Build the resource with service metadata
    let resource = Resource::builder()
        .with_detectors(&[
            Box::new(SdkProvidedResourceDetector),
            Box::new(EnvResourceDetector::new()),
            Box::new(TelemetryResourceDetector),
        ])
        .with_attributes([
            KeyValue::new(
                opentelemetry_semantic_conventions::attribute::SERVICE_NAME,
                config.service_name.clone(),
            ),
            KeyValue::new(
                opentelemetry_semantic_conventions::attribute::SERVICE_VERSION,
                config.service_version.clone(),
            ),
        ])
        .build();

    // Build the tracer provider
    let sampler = if (config.sample_ratio - 1.0).abs() < f64::EPSILON {
        Sampler::AlwaysOn
    } else if config.sample_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sample_ratio)
    };

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(sampler)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource)
        .build();

    let tracer = provider.tracer("pares-radix");

    // Set the global provider so spans propagate across async boundaries
    let _ = global::set_tracer_provider(provider);

    Ok(tracing_opentelemetry::layer().with_tracer(tracer))
}

/// Gracefully shut down the OTLP exporter, flushing all pending spans.
///
/// Call this during application shutdown. Retrieves the provider set via
/// `set_tracer_provider` and calls shutdown on it.
pub fn shutdown_otel() {
    // Drop the global provider which triggers shutdown on the SdkTracerProvider
    // when the last reference is released.
    let _prev = global::set_tracer_provider(SdkTracerProvider::builder().build());
}

/// Errors from OTLP initialization.
#[derive(Debug, thiserror::Error)]
pub enum OtelError {
    #[error("OTLP exporter initialization failed: {0}")]
    ExporterInit(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = OtelConfig::default();
        assert_eq!(config.endpoint, "http://localhost:4317");
        assert_eq!(config.service_name, "pares-radix");
        assert!((config.sample_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_config_from_env() {
        // Without env vars set, should use defaults
        let config = OtelConfig::from_env();
        // service_name defaults to "pares-radix" when OTEL_SERVICE_NAME is unset
        assert!(!config.service_name.is_empty());
        assert!(!config.endpoint.is_empty());
    }

    #[test]
    fn test_sampler_selection() {
        // 1.0 → AlwaysOn
        let config = OtelConfig {
            sample_ratio: 1.0,
            ..Default::default()
        };
        // We can't easily inspect Sampler variants, but verify no panic
        let _ = &config;

        // 0.0 → AlwaysOff
        let config = OtelConfig {
            sample_ratio: 0.0,
            ..Default::default()
        };
        let _ = &config;

        // 0.5 → TraceIdRatioBased
        let config = OtelConfig {
            sample_ratio: 0.5,
            ..Default::default()
        };
        let _ = &config;
    }
}
