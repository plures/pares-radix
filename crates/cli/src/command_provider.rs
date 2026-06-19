//! Plugin command surface for the Pares Radix host CLI.
//!
//! This module defines the seam that lets an *external* crate (notably the
//! private `pares-agens` plugin) contribute additional `clap` subcommands to
//! the public `pares-radix` host binary **without** the host depending on the
//! plugin. The host owns the command surface; plugins register into it.
//!
//! ## Direction of the dependency
//!
//! `pares-radix` (public) must **not** depend on `pares-agens` (private). This
//! trait inverts that: `pares-agens` depends on `pares-radix` *as a library*,
//! implements [`CommandProvider`], and composes the final binary by handing its
//! providers to the host at startup (decision **C1**, compile-time composition).
//! Nothing here references any agens type — only `clap`.
//!
//! ## How it works with `clap` derive
//!
//! The host CLI is built with `clap` *derive* (`#[derive(Parser)]` on `Cli`).
//! A derive app exposes its underlying builder [`clap::Command`] via
//! [`clap::CommandFactory::command`]. A provider:
//!
//! 1. **Augments** that `Command` with its own subcommands
//!    ([`CommandProvider::augment`]), then the host parses argv into
//!    [`clap::ArgMatches`].
//! 2. **Handles** a matched top-level subcommand by name
//!    ([`CommandProvider::handle`]). If the name is not one of the provider's
//!    subcommands it returns [`ProviderOutcome::NotHandled`] so the host can
//!    fall through to its own derived `Commands` match.
//!
//! This is clap's standard "external/dynamic subcommand" mechanism
//! ([`clap::Command::subcommand`] + [`clap::ArgMatches::subcommand`]) expressed
//! as a small object-safe trait.
//!
//! Stage 1 only establishes the trait + registry so Stage 2 has a stable target
//! to register `ServeSpine`, the model router, and `bitnet` against. No agent
//! commands are moved here yet.

use std::fmt;

use async_trait::async_trait;

/// Error returned by a [`CommandProvider`] while handling a subcommand.
///
/// Providers live in other crates and have their own error types, so the host
/// only sees a boxed, type-erased error. Construct one with [`CommandError::new`]
/// or via the `From<E>` impl for any `std::error::Error`.
#[derive(Debug, thiserror::Error)]
#[error("command provider error: {0}")]
pub struct CommandError(Box<dyn std::error::Error + Send + Sync + 'static>);

impl CommandError {
    /// Wrap any error type into a [`CommandError`].
    pub fn new<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self(Box::new(err))
    }

    /// Wrap an arbitrary message as a [`CommandError`].
    pub fn msg(message: impl Into<String>) -> Self {
        Self(Box::<dyn std::error::Error + Send + Sync>::from(
            message.into(),
        ))
    }
}

impl From<Box<dyn std::error::Error + Send + Sync + 'static>> for CommandError {
    fn from(err: Box<dyn std::error::Error + Send + Sync + 'static>) -> Self {
        Self(err)
    }
}

/// Convenience alias for results returned by provider handlers.
pub type CommandResult = Result<(), CommandError>;

/// Outcome of asking a provider to handle a parsed top-level subcommand.
///
/// Making "I handled it" vs. "not mine" explicit (instead of `Result<Option<_>>`)
/// keeps the host dispatch loop unambiguous: the host tries each provider in
/// turn and stops at the first [`ProviderOutcome::Handled`].
#[derive(Debug)]
pub enum ProviderOutcome {
    /// The provider owns this subcommand and ran it (with success or failure).
    Handled(CommandResult),
    /// The provider does not own this subcommand; the host should keep looking.
    NotHandled,
}

impl ProviderOutcome {
    /// `true` if this outcome claimed the subcommand (regardless of success).
    pub fn is_handled(&self) -> bool {
        matches!(self, ProviderOutcome::Handled(_))
    }
}

/// A unit of plugin command surface contributed by an external crate.
///
/// Implementors attach their `clap` subcommands in [`augment`](Self::augment)
/// and execute a matched subcommand in [`handle`](Self::handle). The trait is
/// object-safe so the host can hold `Box<dyn CommandProvider>` without knowing
/// the concrete type, and `async` (via `async-trait`) because real agent
/// commands such as `ServeSpine` are asynchronous.
#[async_trait]
pub trait CommandProvider: Send + Sync {
    /// Stable identifier for this provider, used in diagnostics/logging.
    fn name(&self) -> &str;

    /// Attach this provider's subcommands to the host's `clap` command.
    ///
    /// Receives the (possibly already-augmented) host [`clap::Command`] and
    /// returns it with additional subcommands attached, e.g.:
    ///
    /// ```ignore
    /// fn augment(&self, cmd: clap::Command) -> clap::Command {
    ///     cmd.subcommand(clap::Command::new("serve-spine")
    ///         .about("Run the agent pipeline"))
    /// }
    /// ```
    fn augment(&self, cmd: clap::Command) -> clap::Command;

    /// Handle a matched top-level subcommand by `name`.
    ///
    /// `matches` is the *subcommand's* [`clap::ArgMatches`] (i.e. the value from
    /// [`clap::ArgMatches::subcommand`]). Return [`ProviderOutcome::Handled`] if
    /// `name` is one of this provider's subcommands, otherwise
    /// [`ProviderOutcome::NotHandled`].
    async fn handle(&self, name: &str, matches: &clap::ArgMatches) -> ProviderOutcome;
}

/// Ordered collection of [`CommandProvider`]s composed into the host CLI.
///
/// This is the concrete registration entrypoint the plugin (`pares-agens`) uses
/// at startup: it builds its providers and hands them to the host, which calls
/// [`augment_all`](Self::augment_all) before parsing and
/// [`dispatch`](Self::dispatch) after parsing. The host never names a plugin
/// type — only this registry and the trait object.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: Vec<Box<dyn CommandProvider>>,
}

impl ProviderRegistry {
    /// Create an empty registry (host runs with zero plugin commands).
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Register a provider. Returns `self` for builder-style chaining.
    pub fn register(mut self, provider: Box<dyn CommandProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    /// `true` if no providers are registered.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// Number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Apply every provider's [`CommandProvider::augment`] to `cmd`, in order.
    pub fn augment_all(&self, mut cmd: clap::Command) -> clap::Command {
        for provider in &self.providers {
            cmd = provider.augment(cmd);
        }
        cmd
    }

    /// Offer a matched subcommand to each provider until one claims it.
    ///
    /// Returns the claiming provider's [`CommandResult`], or `None` if no
    /// provider owns `name` (the host should then try its own derived commands).
    pub async fn dispatch(
        &self,
        name: &str,
        matches: &clap::ArgMatches,
    ) -> Option<CommandResult> {
        for provider in &self.providers {
            match provider.handle(name, matches).await {
                ProviderOutcome::Handled(result) => return Some(result),
                ProviderOutcome::NotHandled => continue,
            }
        }
        None
    }
}

impl fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.providers.iter().map(|p| p.name()).collect();
        f.debug_struct("ProviderRegistry")
            .field("providers", &names)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway provider that owns a single `ping` subcommand. This proves
    /// the trait is object-safe, augments derive-built commands, and dispatches
    /// — without any agens dependency.
    struct PingProvider;

    #[async_trait]
    impl CommandProvider for PingProvider {
        fn name(&self) -> &str {
            "ping-provider"
        }

        fn augment(&self, cmd: clap::Command) -> clap::Command {
            cmd.subcommand(clap::Command::new("ping").about("Reply with pong"))
        }

        async fn handle(&self, name: &str, _matches: &clap::ArgMatches) -> ProviderOutcome {
            if name == "ping" {
                ProviderOutcome::Handled(Ok(()))
            } else {
                ProviderOutcome::NotHandled
            }
        }
    }

    fn base_command() -> clap::Command {
        // Stand-in for the derive-built host command (Cli::command()).
        clap::Command::new("pares-radix").subcommand(clap::Command::new("migrate"))
    }

    #[test]
    fn registry_augments_external_subcommand() {
        let registry = ProviderRegistry::new().register(Box::new(PingProvider));
        let cmd = registry.augment_all(base_command());
        let matches = cmd
            .try_get_matches_from(["pares-radix", "ping"])
            .expect("ping subcommand should parse after augmentation");
        assert_eq!(matches.subcommand_name(), Some("ping"));
    }

    #[tokio::test]
    async fn registry_dispatches_to_owning_provider() {
        let registry = ProviderRegistry::new().register(Box::new(PingProvider));
        let empty = clap::ArgMatches::default();

        let claimed = registry.dispatch("ping", &empty).await;
        assert!(claimed.is_some(), "provider should claim its own subcommand");
        assert!(claimed.unwrap().is_ok());

        let unclaimed = registry.dispatch("migrate", &empty).await;
        assert!(
            unclaimed.is_none(),
            "host commands must fall through (NotHandled)"
        );
    }

    #[test]
    fn empty_registry_is_noop() {
        let registry = ProviderRegistry::new();
        assert!(registry.is_empty());
        let cmd = registry.augment_all(base_command());
        // Host's own subcommand still works; nothing added.
        assert!(cmd.try_get_matches_from(["pares-radix", "migrate"]).is_ok());
    }
}
