//! Interactive tool-approval round-trip (block-and-await).
//!
//! When [`crate::tool_governance::GovernanceVerdict::AllowWithApprovalWarning`]
//! is returned for a tool call, the caller can register a **pending approval**
//! here instead of proceeding blindly. Registration yields:
//!
//! * a [`PendingApproval`] — an awaitable future that resolves to an
//!   [`ApprovalDecision`] once a user presses Allow/Deny, and
//! * a `token` string that identifies this pending request.
//!
//! A channel adapter (Telegram, CLI, …) renders an Allow/Deny card carrying the
//! `token` in its callback data. When the user presses a button, the adapter
//! calls [`ApprovalRegistry::resolve`] with the token and decision, which wakes
//! the awaiting tool call.
//!
//! # Channel-agnostic (C-TEST-002)
//!
//! The block-and-await logic lives entirely here. No adapter is required to
//! exercise it: a unit test can `register`, `resolve`, and await the decision
//! with no Telegram/CLI in the loop. Adapters only render the card and route
//! the callback token back into [`ApprovalRegistry::resolve`].

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

/// The user's decision on a pending tool approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// User pressed Allow — the tool call should proceed.
    Allow,
    /// User pressed Deny — the tool call must be aborted.
    Deny,
}

impl ApprovalDecision {
    /// Whether execution should proceed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, ApprovalDecision::Allow)
    }
}

/// An awaitable handle to a pending approval.
///
/// Awaiting [`PendingApproval::wait`] blocks until the matching
/// [`ApprovalRegistry::resolve`] fires (or the registry is dropped). A dropped
/// sender (e.g. the card was dismissed and the registry cleared) resolves to
/// [`ApprovalDecision::Deny`] — fail-closed.
pub struct PendingApproval {
    /// Stable identifier the adapter embeds in the Allow/Deny callback data.
    pub token: String,
    rx: oneshot::Receiver<ApprovalDecision>,
}

impl PendingApproval {
    /// Block until the user resolves this approval.
    ///
    /// Fail-closed: if the sender was dropped without a decision, returns
    /// [`ApprovalDecision::Deny`].
    pub async fn wait(self) -> ApprovalDecision {
        self.rx.await.unwrap_or(ApprovalDecision::Deny)
    }
}

/// A short human-facing summary of what is being approved, carried alongside
/// the token so the adapter can render a meaningful card.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    /// Token identifying the pending approval (matches [`PendingApproval::token`]).
    pub token: String,
    /// Tool name being gated (e.g. `"run_command"`).
    pub tool_name: String,
    /// Short description / arguments preview for the card body.
    pub summary: String,
}

#[derive(Default)]
struct RegistryInner {
    waiters: HashMap<String, oneshot::Sender<ApprovalDecision>>,
}

/// Registry of in-flight tool approvals, keyed by token.
///
/// Cheaply cloneable (`Arc` inside); share one instance between the tool
/// executor and the channel adapter's callback handler.
#[derive(Clone, Default)]
pub struct ApprovalRegistry {
    inner: Arc<Mutex<RegistryInner>>,
}

impl ApprovalRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new pending approval for `tool_name` / `summary`.
    ///
    /// Returns the [`ApprovalRequest`] (to hand to the adapter for rendering)
    /// and the [`PendingApproval`] the caller awaits. The token is a fresh
    /// UUID, so tokens never collide across concurrent tool calls.
    pub async fn register(
        &self,
        tool_name: &str,
        summary: &str,
    ) -> (ApprovalRequest, PendingApproval) {
        let token = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        {
            let mut inner = self.inner.lock().await;
            inner.waiters.insert(token.clone(), tx);
        }
        let request = ApprovalRequest {
            token: token.clone(),
            tool_name: tool_name.to_string(),
            summary: summary.to_string(),
        };
        let pending = PendingApproval { token, rx };
        (request, pending)
    }

    /// Resolve a pending approval by token with the user's decision.
    ///
    /// Returns `true` if a matching waiter existed and was woken, `false` if
    /// the token was unknown or already resolved (idempotent / safe to call on
    /// stale callback presses).
    pub async fn resolve(&self, token: &str, decision: ApprovalDecision) -> bool {
        let sender = {
            let mut inner = self.inner.lock().await;
            inner.waiters.remove(token)
        };
        match sender {
            Some(tx) => tx.send(decision).is_ok(),
            None => false,
        }
    }

    /// Number of currently pending approvals (diagnostics / tests).
    pub async fn pending_count(&self) -> usize {
        self.inner.lock().await.waiters.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deny_aborts_the_tool() {
        let registry = ApprovalRegistry::new();
        let (req, pending) = registry.register("run_command", "rm -rf ./build").await;
        assert_eq!(registry.pending_count().await, 1);

        // Adapter presses Deny using the token from the card.
        let woke = registry.resolve(&req.token, ApprovalDecision::Deny).await;
        assert!(woke, "resolve must wake the waiter");

        let decision = pending.wait().await;
        assert_eq!(decision, ApprovalDecision::Deny);
        assert!(!decision.is_allowed(), "Deny must abort the tool");
        assert_eq!(
            registry.pending_count().await,
            0,
            "waiter removed after resolve"
        );
    }

    #[tokio::test]
    async fn allow_proceeds() {
        let registry = ApprovalRegistry::new();
        let (req, pending) = registry.register("run_command", "cargo build").await;

        // Resolve concurrently so wait() is genuinely block-and-await.
        let reg2 = registry.clone();
        let token = req.token.clone();
        let resolver =
            tokio::spawn(async move { reg2.resolve(&token, ApprovalDecision::Allow).await });

        let decision = pending.wait().await;
        assert!(resolver.await.unwrap(), "resolve reported a woken waiter");
        assert_eq!(decision, ApprovalDecision::Allow);
        assert!(decision.is_allowed(), "Allow must proceed");
    }

    #[tokio::test]
    async fn unknown_token_is_noop() {
        let registry = ApprovalRegistry::new();
        let woke = registry
            .resolve("no-such-token", ApprovalDecision::Allow)
            .await;
        assert!(!woke, "unknown token must not report a woken waiter");
    }

    #[tokio::test]
    async fn dropped_registry_fails_closed_to_deny() {
        let (_req, pending) = {
            let registry = ApprovalRegistry::new();
            let (req, pending) = registry.register("run_command", "danger").await;
            (req, pending)
            // registry dropped here → sender dropped
        };
        // No resolve ever happens; the sender is gone.
        let decision = pending.wait().await;
        assert_eq!(
            decision,
            ApprovalDecision::Deny,
            "fail-closed on dropped sender"
        );
    }

    #[tokio::test]
    async fn concurrent_tokens_do_not_collide() {
        let registry = ApprovalRegistry::new();
        let (r1, p1) = registry.register("run_command", "one").await;
        let (r2, p2) = registry.register("run_command", "two").await;
        assert_ne!(r1.token, r2.token);
        assert_eq!(registry.pending_count().await, 2);

        // Resolve the second one Allow, first one Deny.
        assert!(registry.resolve(&r2.token, ApprovalDecision::Allow).await);
        assert!(registry.resolve(&r1.token, ApprovalDecision::Deny).await);
        assert_eq!(p1.wait().await, ApprovalDecision::Deny);
        assert_eq!(p2.wait().await, ApprovalDecision::Allow);
    }
}
