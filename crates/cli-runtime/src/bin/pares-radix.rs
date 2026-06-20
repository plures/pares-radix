//! `pares-radix` - the public host binary.
//!
//! This is a thin composition wrapper over the reusable runtime in
//! [`pares_radix_cli_runtime`]. It runs the host with NO plugin providers.
//! The private `pares-agens` plugin composes its own binary that calls
//! [`pares_radix_cli_runtime::run_with_providers`] with a populated registry
//! (decision C1, compile-time composition).

#[tokio::main]
async fn main() {
    pares_radix_cli_runtime::run_with_providers(
        pares_radix_cli_runtime::ProviderRegistry::new(),
    )
    .await;
}
