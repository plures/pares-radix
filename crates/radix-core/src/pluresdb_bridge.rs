//! Platform bridge to the PluresDB procedure engine.
//!
//! This module owns the **procedure / constraint execution** half of the
//! PluresDB bridge. It holds an in-process [`CrdtStore`] snapshot plus a
//! named-procedure registry and exposes:
//!
//! - [`PluresDbBridge::run_steps`] — execute a raw pipeline of [`Step`]s.
//! - [`PluresDbBridge::run_procedure`] — execute a named procedure by looking
//!   up its DSL string in the in-process procedure registry.
//! - [`PluresDbBridge::load_constraints`] — load praxis constraints stored in
//!   PluresDB.
//!
//! It is deliberately **platform-only**: it knows nothing about the pares-radix
//! cognition layer (the `memory` / `cerebellum` modules). The cognition seam
//! — snapshotting a `MemoryStore` into a [`CrdtStore`] — lives on the cognition
//! side (`crate::cerebellum::bridge::StoreAdapter`), which constructs this
//! bridge via [`PluresDbBridge::from_crdt`].
//!
//! # Example
//!
//! ```rust,no_run
//! # use pares_radix_core::pluresdb_bridge::{PluresDbBridge, BridgeError};
//! # use pares_radix_core::CrdtStore;
//! # fn main() -> Result<(), BridgeError> {
//! let crdt = CrdtStore::default();
//! let bridge = PluresDbBridge::from_crdt(crdt);
//!
//! use pluresdb_procedures::ir::{Predicate, Step};
//! let steps = vec![
//!     Step::Filter { predicate: Predicate::eq("category", "decision") },
//! ];
//! # tokio_test_block(async {
//! let result = bridge.run_steps(steps).await?;
//! println!("{} nodes returned", result.nodes.len());
//! # Ok::<(), BridgeError>(())
//! # }).unwrap();
//! # Ok(())
//! # }
//! # fn tokio_test_block<F: std::future::Future>(_f: F) -> Result<(), BridgeError> { Ok(()) }
//! ```

use std::collections::HashMap;

use pluresdb::CrdtStore;
use pluresdb_procedures::{
    engine::ProcedureEngine,
    ir::{ProcedureResult, Step},
};

// ---------------------------------------------------------------------------
// BridgeError
// ---------------------------------------------------------------------------

/// Errors that can occur when using the PluresDB bridge.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// A named procedure was requested but could not be found in the registry.
    #[error("procedure not found: {0}")]
    NotFound(String),
    /// The procedure engine reported an execution failure.
    #[error("execution failed: {0}")]
    Execution(String),
    /// The underlying memory store returned an error.
    #[error("store error: {0}")]
    Store(String),
}

// ---------------------------------------------------------------------------
// PluresDbBridge
// ---------------------------------------------------------------------------

/// Platform bridge between pares-radix and the PluresDB procedure engine.
///
/// The bridge maintains:
/// - An in-process [`CrdtStore`] snapshot.
/// - A named-procedure registry (mapping procedure names to DSL strings).
///
/// It is constructed from an already-built [`CrdtStore`] via
/// [`PluresDbBridge::from_crdt`]. The cognition layer is responsible for
/// producing that snapshot (e.g. from a `MemoryStore`); this type is agnostic
/// to where the [`CrdtStore`] came from.
pub struct PluresDbBridge {
    crdt: CrdtStore,
    /// Named procedure registry: maps a procedure name to its DSL string.
    procedures: HashMap<String, String>,
}

impl PluresDbBridge {
    /// Create a bridge from an already-populated [`CrdtStore`].
    ///
    /// This is the platform constructor seam: callers (including the cognition
    /// `StoreAdapter`) build a [`CrdtStore`] and hand it here.
    pub fn from_crdt(crdt: CrdtStore) -> Self {
        Self {
            crdt,
            procedures: HashMap::new(),
        }
    }

    /// Replace the internal [`CrdtStore`] snapshot with a freshly-built one.
    ///
    /// The registered procedure DSL table is preserved across the reload.
    pub fn reload_from(&mut self, crdt: CrdtStore) {
        self.crdt = crdt;
    }

    /// Register a named procedure DSL string.
    ///
    /// Replaces any existing registration for `name`.
    pub fn register_procedure(&mut self, name: impl Into<String>, dsl: impl Into<String>) {
        self.procedures.insert(name.into(), dsl.into());
    }

    /// Run a named procedure pipeline.
    ///
    /// Looks up `name` in the registered procedure DSL table and executes the
    /// corresponding DSL string against the current [`CrdtStore`] snapshot.
    ///
    /// # Errors
    ///
    /// - [`BridgeError::NotFound`] if `name` is not registered.
    /// - [`BridgeError::Execution`] if the DSL parse or execution fails.
    pub async fn run_procedure(&self, name: &str) -> Result<ProcedureResult, BridgeError> {
        let dsl = self
            .procedures
            .get(name)
            .ok_or_else(|| BridgeError::NotFound(name.to_string()))?;

        let engine = ProcedureEngine::new(&self.crdt, "pares-radix");
        engine
            .exec_dsl(dsl)
            .map_err(|e| BridgeError::Execution(e.to_string()))
    }

    /// Run a raw pipeline of [`Step`]s against the current [`CrdtStore`] snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError::Execution`] if the pipeline fails.
    pub async fn run_steps(&self, steps: Vec<Step>) -> Result<ProcedureResult, BridgeError> {
        let engine = ProcedureEngine::new(&self.crdt, "pares-radix");
        engine
            .exec(&steps)
            .map_err(|e| BridgeError::Execution(e.to_string()))
    }

    /// Load praxis constraints stored in PluresDB.
    ///
    /// Queries for nodes with type `praxis:constraint` and deserializes them
    /// into [`Constraint`][pares_radix_praxis::db::schema::Constraint] records.
    /// Returns an empty vec if no constraints are stored (the caller should
    /// merge with seed constraints).
    pub fn load_constraints(
        &self,
    ) -> Result<Vec<pares_radix_praxis::db::schema::Constraint>, BridgeError> {
        use pluresdb_procedures::ir::{Predicate, Step};

        let steps = vec![Step::Filter {
            predicate: Predicate::eq("type", "praxis:constraint"),
        }];

        let engine = ProcedureEngine::new(&self.crdt, "pares-radix");
        let result = engine
            .exec(&steps)
            .map_err(|e| BridgeError::Execution(e.to_string()))?;

        let mut constraints = Vec::new();
        for node in &result.nodes {
            // Each node is a serde_json::Value; try to deserialize the constraint data
            match serde_json::from_value::<pares_radix_praxis::db::schema::Constraint>(node.clone())
            {
                Ok(c) => constraints.push(c),
                Err(e) => {
                    tracing::warn!("failed to deserialize praxis constraint from node: {e}");
                }
            }
        }

        Ok(constraints)
    }
}

// ---------------------------------------------------------------------------
// Tests (platform: CrdtStore-driven, no MemoryStore)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use pluresdb_procedures::ir::{Predicate, SortDir, Step};

    /// Build a [`CrdtStore`] pre-populated with the same shape the cognition
    /// `StoreAdapter` would produce, so platform execution can be exercised
    /// without touching the cognition memory layer.
    fn crdt_with_entries() -> CrdtStore {
        let crdt = CrdtStore::default();
        let mk = |_id: &str, content: &str, category: &str| {
            serde_json::json!({
                "category": category,
                "content": content,
                "score": 0.9_f64,
                "tags": Vec::<String>::new(),
                "created_at": "2026-01-01T00:00:00Z",
            })
        };
        crdt.put("d1", "pares-radix", mk("d1", "Use tokio for async", "decision"));
        crdt.put("d2", "pares-radix", mk("d2", "Avoid blocking calls", "decision"));
        crdt.put("c1", "pares-radix", mk("c1", "fn main() {}", "code-pattern"));
        crdt
    }

    fn bridge_with_entries() -> PluresDbBridge {
        PluresDbBridge::from_crdt(crdt_with_entries())
    }

    // ── PluresDbBridge::run_steps ─────────────────────────────────────────────

    #[tokio::test]
    async fn run_steps_filter_by_category() {
        let bridge = bridge_with_entries();
        let steps = vec![Step::Filter {
            predicate: Predicate::eq("category", "decision"),
        }];
        let result = bridge.run_steps(steps).await.unwrap();
        assert_eq!(result.nodes.len(), 2, "expected 2 decision entries");
    }

    #[tokio::test]
    async fn run_steps_filter_sort_limit() {
        let bridge = bridge_with_entries();
        let steps = vec![
            Step::Filter {
                predicate: Predicate::eq("category", "decision"),
            },
            Step::Sort {
                by: "score".to_string(),
                dir: SortDir::Desc,
                after: None,
            },
            Step::Limit { n: 1 },
        ];
        let result = bridge.run_steps(steps).await.unwrap();
        assert_eq!(result.nodes.len(), 1);
    }

    #[tokio::test]
    async fn run_steps_empty_pipeline_returns_all() {
        let bridge = bridge_with_entries();
        let result = bridge.run_steps(vec![]).await.unwrap();
        assert_eq!(result.nodes.len(), 3);
    }

    // ── PluresDbBridge::run_procedure ─────────────────────────────────────────

    #[tokio::test]
    async fn run_procedure_not_found_returns_error() {
        let bridge = bridge_with_entries();
        let err = bridge.run_procedure("unknown-proc").await.unwrap_err();
        assert!(
            matches!(err, BridgeError::NotFound(_)),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn run_procedure_registered_dsl_executes() {
        let mut bridge = bridge_with_entries();
        bridge.register_procedure(
            "recall-decisions",
            r#"filter(category == "decision") |> limit(10)"#,
        );
        let result = bridge.run_procedure("recall-decisions").await.unwrap();
        assert_eq!(result.nodes.len(), 2);
    }

    #[tokio::test]
    async fn run_procedure_invalid_dsl_returns_execution_error() {
        let mut bridge = bridge_with_entries();
        bridge.register_procedure("bad", "this is not valid DSL !!!");
        let err = bridge.run_procedure("bad").await.unwrap_err();
        assert!(
            matches!(err, BridgeError::Execution(_)),
            "unexpected error: {err}"
        );
    }

    // ── PluresDbBridge::reload_from ───────────────────────────────────────────

    #[tokio::test]
    async fn reload_from_replaces_snapshot_preserving_procedures() {
        let mut bridge = bridge_with_entries();
        bridge.register_procedure(
            "recall-decisions",
            r#"filter(category == "decision") |> limit(10)"#,
        );

        // Replace with an empty snapshot.
        bridge.reload_from(CrdtStore::default());
        let after = bridge.run_steps(vec![]).await.unwrap();
        assert_eq!(after.nodes.len(), 0);

        // Procedure registration survives the reload (resolves, executes on empty store).
        let result = bridge.run_procedure("recall-decisions").await.unwrap();
        assert_eq!(result.nodes.len(), 0);
    }
}
