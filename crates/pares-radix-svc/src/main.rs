//! Binary entrypoint for `pares-radix-svc` — headless service runtime.
//!
//! Reads config from environment (`RADIX_SVC_*`, see
//! [`pares_radix_svc::ServiceConfig::from_env`]), opens the PluresDB store,
//! starts the scheduler + HTTP automation surface, and runs until Ctrl-C
//! (SIGINT) or SIGTERM, then drains cleanly.

use pares_radix_svc::{ctrl_c_shutdown, ServiceConfig, ServiceLifecycle};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = ServiceConfig::from_env()?;
    let lifecycle = ServiceLifecycle::new(config)?;

    let shutdown = shutdown_signal();
    lifecycle.run(shutdown).await
}

/// Wait for Ctrl-C or (on Unix) SIGTERM, whichever comes first.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c_shutdown() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        ctrl_c_shutdown().await;
    }
}
