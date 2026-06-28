//! OpenTelemetry OTLP metrics export layer.
//!
//! When the `otel` feature is enabled, this module provides initialization
//! of an OTLP metrics exporter that periodically pushes metrics to a collector.
//!
//! # Configuration
//!
//! Shares configuration with the trace exporter via [`super::otel::OtelConfig`].
//! The metrics endpoint defaults to the same host but uses the standard metrics path:
//! - gRPC: same endpoint as traces (port 4317)
//! - HTTP/protobuf: `{endpoint}/v1/metrics` (port 4318)
//!
//! Additional env vars:
//! - `OTEL_METRIC_EXPORT_INTERVAL` — export interval in milliseconds (default: 60000)
//!
//! # Usage
//!
//! ```no_run
//! # #[cfg(feature = "otel")]
//! # {
//! use pares_radix_core::otel::OtelConfig;
//! use pares_radix_core::otel_metrics::init_metrics;
//!
//! let config = OtelConfig::from_env();
//! let meter_provider = init_metrics(&config).expect("metrics init failed");
//! // meter_provider is set as global — use opentelemetry::global::meter("name")
//! # }
//! ```

use std::time::Duration;

use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::{MetricExporter, WithExportConfig};
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::resource::{
    EnvResourceDetector, SdkProvidedResourceDetector, TelemetryResourceDetector,
};
use opentelemetry_sdk::Resource;

use super::otel::{OtelConfig, OtelError, OtelProtocol};

/// Default metrics export interval (60 seconds).
const DEFAULT_EXPORT_INTERVAL_MS: u64 = 60_000;

/// Metrics-specific configuration layered on top of [`OtelConfig`].
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    /// How often to push metrics to the collector.
    pub export_interval: Duration,
}

impl MetricsConfig {
    /// Load metrics config from environment.
    pub fn from_env() -> Self {
        let interval_ms = std::env::var("OTEL_METRIC_EXPORT_INTERVAL")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_EXPORT_INTERVAL_MS);

        Self {
            export_interval: Duration::from_millis(interval_ms),
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            export_interval: Duration::from_millis(DEFAULT_EXPORT_INTERVAL_MS),
        }
    }
}

/// Initialize the OTLP metrics pipeline and set the global meter provider.
///
/// Returns the [`SdkMeterProvider`] handle. Call [`shutdown_metrics`] on
/// graceful shutdown to flush pending metric data.
pub fn init_metrics(config: &OtelConfig) -> Result<SdkMeterProvider, OtelError> {
    init_metrics_with(config, &MetricsConfig::from_env())
}

/// Initialize metrics with explicit metrics-specific config.
pub fn init_metrics_with(
    config: &OtelConfig,
    metrics_config: &MetricsConfig,
) -> Result<SdkMeterProvider, OtelError> {
    // Build the OTLP metric exporter based on configured protocol
    let exporter = match config.protocol {
        OtelProtocol::Grpc => MetricExporter::builder()
            .with_tonic()
            .with_endpoint(&config.endpoint)
            .build()
            .map_err(|e| OtelError::ExporterInit(format!("metrics gRPC: {e}")))?,
        OtelProtocol::HttpProto => MetricExporter::builder()
            .with_http()
            .with_endpoint(&config.endpoint)
            .build()
            .map_err(|e| OtelError::ExporterInit(format!("metrics HTTP: {e}")))?,
    };

    // Build a periodic reader that pushes metrics at the configured interval
    let reader = PeriodicReader::builder(exporter)
        .with_interval(metrics_config.export_interval)
        .build();

    // Build the resource with service metadata (same as traces)
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

    // Build and install the meter provider
    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resource)
        .build();

    // Set as global so any code can use `global::meter("name")`
    global::set_meter_provider(provider.clone());

    Ok(provider)
}

/// Gracefully shut down the metrics pipeline, flushing pending data.
///
/// Call this during application shutdown alongside [`super::otel::shutdown_otel`].
pub fn shutdown_metrics(provider: &SdkMeterProvider) -> Result<(), OtelError> {
    provider
        .shutdown()
        .map_err(|e| OtelError::ExporterInit(format!("metrics shutdown: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_config_defaults() {
        let config = MetricsConfig::default();
        assert_eq!(config.export_interval, Duration::from_secs(60));
    }

    #[test]
    fn test_metrics_config_from_env_default() {
        // Without env var, should use default interval
        let config = MetricsConfig::from_env();
        assert!(config.export_interval.as_millis() > 0);
    }

    #[test]
    fn test_metrics_config_custom_interval() {
        let config = MetricsConfig {
            export_interval: Duration::from_secs(15),
        };
        assert_eq!(config.export_interval.as_secs(), 15);
    }

    #[tokio::test]
    async fn test_init_metrics_grpc() {
        // Should succeed even without a collector (exporter is lazy)
        let otel_config = OtelConfig {
            endpoint: "http://localhost:4317".into(),
            protocol: OtelProtocol::Grpc,
            ..Default::default()
        };
        let metrics_config = MetricsConfig {
            export_interval: Duration::from_secs(30),
        };
        let result = init_metrics_with(&otel_config, &metrics_config);
        assert!(result.is_ok());
        // Clean up global state
        if let Ok(provider) = result {
            let _ = shutdown_metrics(&provider);
        }
    }

    #[tokio::test]
    async fn test_init_metrics_http() {
        let otel_config = OtelConfig {
            endpoint: "http://localhost:4318".into(),
            protocol: OtelProtocol::HttpProto,
            ..Default::default()
        };
        let metrics_config = MetricsConfig {
            export_interval: Duration::from_secs(30),
        };
        let result = init_metrics_with(&otel_config, &metrics_config);
        assert!(result.is_ok());
        if let Ok(provider) = result {
            let _ = shutdown_metrics(&provider);
        }
    }

    #[tokio::test]
    async fn test_shutdown_metrics() {
        let otel_config = OtelConfig::default();
        let metrics_config = MetricsConfig::default();
        let provider = init_metrics_with(&otel_config, &metrics_config).unwrap();
        // Shutdown should succeed
        let result = shutdown_metrics(&provider);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_global_meter_available_after_init() {
        let otel_config = OtelConfig::default();
        let metrics_config = MetricsConfig {
            export_interval: Duration::from_secs(300),
        };
        let provider = init_metrics_with(&otel_config, &metrics_config).unwrap();

        // Should be able to create a meter and instrument via global
        let meter = global::meter("test-meter");
        let counter = meter.u64_counter("test_counter").build();
        counter.add(1, &[KeyValue::new("test", "value")]);

        // No panic = success
        let _ = shutdown_metrics(&provider);
    }

    #[tokio::test]
    async fn test_histogram_recording() {
        let otel_config = OtelConfig::default();
        let metrics_config = MetricsConfig {
            export_interval: Duration::from_secs(300),
        };
        let provider = init_metrics_with(&otel_config, &metrics_config).unwrap();

        let meter = global::meter("test-meter-hist");
        let histogram = meter.f64_histogram("request_duration_ms").build();
        histogram.record(42.5, &[KeyValue::new("endpoint", "/api/v1/health")]);

        // No panic = success
        let _ = shutdown_metrics(&provider);
    }

    #[tokio::test]
    async fn test_gauge_recording() {
        let otel_config = OtelConfig::default();
        let metrics_config = MetricsConfig {
            export_interval: Duration::from_secs(300),
        };
        let provider = init_metrics_with(&otel_config, &metrics_config).unwrap();

        let meter = global::meter("test-meter-gauge");
        let gauge = meter.f64_gauge("memory_usage_bytes").build();
        gauge.record(1024.0 * 1024.0 * 512.0, &[KeyValue::new("host", "devbox")]);

        let _ = shutdown_metrics(&provider);
    }
}
