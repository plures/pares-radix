//! Shadow procedure holder — loads umbra-evolved candidate `.px` procedures
//! (`praxis/shadow/*.px`) **without** registering them in the live
//! [`ReactiveRegistry`](crate::spine::reactive::ReactiveRegistry).
//!
//! # Why this exists
//!
//! `pares-umbra` evolves classifiers (routers, intent/priority scorers) and emits
//! them as `.px` candidates. We want those candidates to ride into pares-radix and
//! deploy to praxisbot via the normal `praxis/` sync, but run **only in shadow** —
//! they must never affect live output until they consistently beat the live
//! classifier. The shadow candidates declare `trigger: manual`, so
//! [`register_reactive_procedures`](crate::spine::bootstrap::register_reactive_procedures)
//! already skips them. This holder is the *positive* half: it loads those same
//! candidates into an out-of-band collection so they can be evaluated/promoted
//! later, parallel to (and separate from) the reactive pipeline.
//!
//! # Architecture seam (C-PLURES)
//!
//! The evolutionary arena and fitness accounting live in **umbra**
//! (`umbra_shadow::ShadowArena`, `umbra_fitness`), not here. pares-radix must not
//! host a second evolutionary engine. This holder is intentionally thin: it only
//! *loads* candidates and exposes them. The eventual integration wires these loaded
//! candidates into a `umbra_shadow::ShadowArena` (fed the same real traffic the live
//! classifier sees) which accumulates fitness and signals promotion. Until that
//! wiring lands, the holder is the stable seam: `ShadowProcedures::candidates()`
//! yields the loaded shadow procedures and their model ids for an external scorer.
//!
//! See `praxis/shadow/README.md` and ADR notes in `memory/2026-06-17-umbra-shadow-deploy.md`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tracing::{info, warn};

use crate::px_adapter::{load_px_directory, AsyncActionHandler, PxProcedureAdapter};

/// The trigger kind that marks a procedure as non-reactive (shadow-eligible).
const MANUAL_TRIGGER: &str = "manual";

/// Metadata about a single loaded shadow candidate.
#[derive(Debug, Clone)]
pub struct ShadowCandidate {
    /// The shadow procedure name as declared in the `.px` file
    /// (e.g. `shadow_route_message`).
    pub name: String,
    /// The trigger kind the procedure declared. Always `"manual"` for accepted
    /// candidates (non-manual procedures are rejected on load).
    pub trigger_kind: String,
}

/// Holds umbra-evolved shadow procedures loaded from `praxis/shadow/`.
///
/// This collection is **separate** from the live [`ReactiveRegistry`]. Procedures
/// held here are never dispatched by the inbound/reactive pipeline. They are kept
/// for out-of-band shadow evaluation (and eventual promotion) by umbra.
///
/// [`ReactiveRegistry`]: crate::spine::reactive::ReactiveRegistry
#[derive(Default)]
pub struct ShadowProcedures {
    /// Loaded shadow adapters, keyed by procedure name.
    adapters: HashMap<String, Arc<PxProcedureAdapter>>,
}

impl ShadowProcedures {
    /// Create an empty shadow holder.
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    /// Load shadow candidates from a directory of `.px` files (recursively).
    ///
    /// Only procedures whose declared trigger kind is `manual` are accepted. Any
    /// procedure with a non-manual trigger found under the shadow directory is a
    /// configuration error (it could otherwise leak into the live graph), so it is
    /// **rejected and logged**, never stored. This makes "shadow" a hard guarantee
    /// at load time, not a convention.
    ///
    /// Returns the number of shadow candidates loaded. Safe to call when the
    /// directory does not exist (returns 0).
    pub fn load_dir(&mut self, dir: &Path, handler: Arc<dyn AsyncActionHandler>) -> usize {
        if !dir.is_dir() {
            return 0;
        }

        let adapters = load_px_directory(dir, handler);
        let mut loaded = 0;
        let mut rejected = 0;

        for adapter in adapters {
            let name = adapter.name().to_string();
            let kind = adapter.trigger_kind().to_string();

            if kind != MANUAL_TRIGGER {
                // A non-manual procedure under praxis/shadow/ would be reactive if it
                // were ever loaded into the live registry. Refuse to hold it as a
                // shadow candidate and make the misconfiguration loud.
                warn!(
                    procedure = %name,
                    trigger_kind = %kind,
                    "shadow: refusing non-manual procedure in shadow dir (must be `trigger: manual`)"
                );
                rejected += 1;
                continue;
            }

            self.adapters.insert(name, Arc::new(adapter));
            loaded += 1;
        }

        if rejected > 0 {
            warn!(
                rejected,
                dir = %dir.display(),
                "shadow: rejected non-manual procedures while loading shadow candidates"
            );
        }

        info!(
            loaded,
            dir = %dir.display(),
            "shadow: umbra-evolved candidates loaded (inert; not in live registry)"
        );

        loaded
    }

    /// Number of loaded shadow candidates.
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// Whether the holder has no candidates.
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    /// Whether a shadow procedure with the given name is held.
    pub fn contains(&self, name: &str) -> bool {
        self.adapters.contains_key(name)
    }

    /// Fetch a loaded shadow adapter by name, if present.
    pub fn get(&self, name: &str) -> Option<Arc<PxProcedureAdapter>> {
        self.adapters.get(name).cloned()
    }

    /// Names of all loaded shadow candidates.
    pub fn names(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }

    /// Metadata for all loaded shadow candidates.
    ///
    /// This is the seam an external scorer (umbra `ShadowArena`) consumes to enroll
    /// candidates for fitness accumulation.
    pub fn candidates(&self) -> Vec<ShadowCandidate> {
        self.adapters
            .values()
            .map(|a| ShadowCandidate {
                name: a.name().to_string(),
                trigger_kind: a.trigger_kind().to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::px_adapter::AsyncActionHandler;
    use crate::spine::reactive::ReactiveRegistry;
    use async_trait::async_trait;
    use pares_radix_praxis::px::executor::ExecutionError;
    use serde_json::Value;
    use tempfile::TempDir;

    struct NoOpHandler;

    #[async_trait]
    impl AsyncActionHandler for NoOpHandler {
        async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            Ok(Value::Null)
        }
    }

    /// A minimal shadow-style fixture: manual trigger, non-colliding name.
    const SHADOW_FIXTURE: &str = concat!(
        "procedure shadow_route_message:\n",
        "  trigger: manual\n",
        "  given: \"Umbra-evolved router candidate. Shadow-only.\"\n",
        "  extract_features {content: $content} -> $features\n",
        "  evaluate_shadow_classifier {model: \"route_message\", features: $features} -> $result\n",
        "  emit {shadow: \"route_message\", result: $result}\n",
    );

    const SHADOW_PRIORITY_FIXTURE: &str = concat!(
        "procedure shadow_score_priority:\n",
        "  trigger: manual\n",
        "  extract_features {content: $content} -> $features\n",
        "  evaluate_shadow_classifier {model: \"score_priority\", features: $features} -> $result\n",
        "  emit {shadow: \"score_priority\", result: $result}\n",
    );

    const SHADOW_INTENT_FIXTURE: &str = concat!(
        "procedure shadow_classify_intent:\n",
        "  trigger: manual\n",
        "  extract_features {content: $content} -> $features\n",
        "  evaluate_shadow_classifier {model: \"classify_intent\", features: $features} -> $result\n",
        "  emit {shadow: \"classify_intent\", result: $result}\n",
    );

    fn handler() -> Arc<dyn AsyncActionHandler> {
        Arc::new(NoOpHandler)
    }

    #[test]
    fn load_dir_missing_is_zero() {
        let mut holder = ShadowProcedures::new();
        let n = holder.load_dir(Path::new("/no/such/shadow/dir"), handler());
        assert_eq!(n, 0);
        assert!(holder.is_empty());
    }

    #[test]
    fn loads_three_shadow_candidates_from_fixture() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("shadow_route_message.px"), SHADOW_FIXTURE).unwrap();
        std::fs::write(
            tmp.path().join("shadow_score_priority.px"),
            SHADOW_PRIORITY_FIXTURE,
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("shadow_classify_intent.px"),
            SHADOW_INTENT_FIXTURE,
        )
        .unwrap();

        let mut holder = ShadowProcedures::new();
        let n = holder.load_dir(tmp.path(), handler());

        assert_eq!(n, 3, "expected 3 shadow candidates");
        assert_eq!(holder.len(), 3);
        assert!(holder.contains("shadow_route_message"));
        assert!(holder.contains("shadow_score_priority"));
        assert!(holder.contains("shadow_classify_intent"));

        let cands = holder.candidates();
        assert_eq!(cands.len(), 3);
        assert!(cands.iter().all(|c| c.trigger_kind == "manual"));
    }

    /// The core inertness guarantee: candidates loaded into the shadow holder are
    /// NOT present in the live reactive registry, and registering the same dir
    /// reactively yields zero triggers.
    #[tokio::test]
    async fn shadow_candidates_are_not_in_live_registry() {
        use crate::spine::bootstrap::register_reactive_procedures;

        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("shadow_route_message.px"), SHADOW_FIXTURE).unwrap();
        std::fs::write(
            tmp.path().join("shadow_score_priority.px"),
            SHADOW_PRIORITY_FIXTURE,
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("shadow_classify_intent.px"),
            SHADOW_INTENT_FIXTURE,
        )
        .unwrap();

        // Shadow holder loads all three.
        let mut holder = ShadowProcedures::new();
        let loaded = holder.load_dir(tmp.path(), handler());
        assert_eq!(loaded, 3);

        // The live reactive registry, fed the SAME directory, registers nothing
        // (manual procedures are skipped).
        let registry = ReactiveRegistry::new();
        let registered = register_reactive_procedures(tmp.path(), &registry, handler()).await;
        assert_eq!(registered, 0, "shadow procedures must not register reactively");
        assert_eq!(
            registry.trigger_count().await,
            0,
            "live registry must have zero triggers from shadow dir"
        );
    }

    /// A non-manual procedure dropped into the shadow dir must be rejected, never
    /// held as a shadow candidate (defense against a candidate leaking into live).
    #[test]
    fn rejects_non_manual_procedure() {
        let tmp = TempDir::new().unwrap();
        // on_write trigger => reactive; must be refused by the shadow holder.
        let reactive = concat!(
            "procedure shadow_but_reactive:\n",
            "  trigger: on_write\n",
            "  emit {oops: \"live\"}\n",
        );
        std::fs::write(tmp.path().join("bad.px"), reactive).unwrap();

        let mut holder = ShadowProcedures::new();
        let n = holder.load_dir(tmp.path(), handler());
        assert_eq!(n, 0, "non-manual procedure must not be accepted");
        assert!(!holder.contains("shadow_but_reactive"));
    }
}
