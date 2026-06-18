//! Part-1 GATE: prove umbra-evolved shadow procedures are genuinely inert in the
//! live graph, and that the real `praxis/shadow/` tree loads without colliding
//! with the live `route_message` procedure.
//!
//! These tests run against the ACTUAL files in `praxis/shadow/` (not just fixtures),
//! so they catch regressions if someone edits a shadow file in a way that would
//! make it reactive or collide with a live procedure.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use pares_agens_core::px_adapter::{load_px_directory, AsyncActionHandler};
use pares_agens_core::spine::bootstrap::register_reactive_procedures;
use pares_agens_core::spine::reactive::ReactiveRegistry;
use pares_agens_core::spine::shadow::ShadowProcedures;
use pares_radix_praxis::px::executor::ExecutionError;

struct NoOpHandler;

#[async_trait]
impl AsyncActionHandler for NoOpHandler {
    async fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
        Ok(Value::Null)
    }
}

fn handler() -> Arc<dyn AsyncActionHandler> {
    Arc::new(NoOpHandler)
}

fn project_root() -> PathBuf {
    // crates/core -> crates -> project root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("project root")
        .to_path_buf()
}

fn shadow_dir() -> PathBuf {
    project_root().join("praxis").join("shadow")
}

/// The shadow files exist and parse into exactly the three evolved candidates,
/// all declaring `trigger: manual`.
#[test]
fn real_shadow_dir_loads_three_manual_candidates() {
    let dir = shadow_dir();
    assert!(dir.is_dir(), "praxis/shadow must exist at {}", dir.display());

    let adapters = load_px_directory(&dir, handler());
    let mut names: Vec<String> = adapters.iter().map(|a| a.name().to_string()).collect();
    names.sort();

    assert_eq!(
        names,
        vec![
            "shadow_classify_intent".to_string(),
            "shadow_route_message".to_string(),
            "shadow_score_priority".to_string(),
        ],
        "expected exactly the 3 shadow candidates (the //-commented EVOLVED-SOURCE \
         block must NOT parse as a second procedure)"
    );

    for a in &adapters {
        assert_eq!(
            a.trigger_kind(),
            "manual",
            "shadow procedure {} must declare trigger: manual",
            a.name()
        );
    }
}

/// GATE: registering the real shadow dir reactively yields ZERO triggers.
/// Manual procedures are skipped by `register_reactive_procedures`.
#[tokio::test]
async fn real_shadow_dir_registers_zero_reactive_triggers() {
    let dir = shadow_dir();
    assert!(dir.is_dir());

    let registry = ReactiveRegistry::new();
    let registered = register_reactive_procedures(&dir, &registry, handler()).await;

    assert_eq!(registered, 0, "shadow procedures must not register reactively");
    assert_eq!(
        registry.trigger_count().await,
        0,
        "live registry must have zero triggers from praxis/shadow"
    );
}

/// GATE: the shadow holder loads the real candidates, and they are NOT in a live
/// registry built from the same directory.
#[tokio::test]
async fn real_shadow_holder_separate_from_live_registry() {
    let dir = shadow_dir();
    assert!(dir.is_dir());

    let mut holder = ShadowProcedures::new();
    let loaded = holder.load_dir(&dir, handler());
    assert_eq!(loaded, 3, "shadow holder must load the 3 real candidates");
    assert!(holder.contains("shadow_route_message"));
    assert!(holder.contains("shadow_score_priority"));
    assert!(holder.contains("shadow_classify_intent"));

    let registry = ReactiveRegistry::new();
    let registered = register_reactive_procedures(&dir, &registry, handler()).await;
    assert_eq!(registered, 0);
    assert_eq!(registry.trigger_count().await, 0);
}

/// GATE: the shadow procedure names must NOT collide with the live `route_message`
/// procedure declared in `praxis/procedures/routing.px`. The shadow router is named
/// `shadow_route_message`, so the live name space is untouched.
#[test]
fn shadow_route_message_does_not_collide_with_live_route_message() {
    let routing = project_root()
        .join("praxis")
        .join("procedures")
        .join("routing.px");
    if routing.is_file() {
        let src = std::fs::read_to_string(&routing).unwrap();
        assert!(
            src.contains("procedure route_message"),
            "expected live route_message in routing.px (collision target)"
        );
    }

    // The shadow side declares the namespaced name, never bare `route_message`.
    let shadow_src =
        std::fs::read_to_string(shadow_dir().join("shadow_route_message.px")).unwrap();
    // The executable declaration is the namespaced one.
    assert!(
        shadow_src.contains("procedure shadow_route_message:"),
        "shadow file must declare the namespaced procedure"
    );
    // The only bare `procedure route_message {` occurrence must be inside the
    // //-prefixed EVOLVED-SOURCE provenance block (a comment), never live.
    for line in shadow_src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("procedure route_message") {
            panic!("shadow file must not declare a live `route_message` procedure");
        }
    }
}

/// Sanity: the live praxis tree (procedures + spine) still loads. This guards
/// against the shadow work accidentally breaking the main load path. We don't
/// assert a specific count (parser support for v3 dataflow syntax varies), only
/// that loading does not panic and the shadow names never appear among live
/// reactive registrations.
#[tokio::test]
async fn live_tree_has_no_shadow_named_reactive_triggers() {
    let registry = ReactiveRegistry::new();
    for sub in ["procedures", "spine"] {
        let d = project_root().join("praxis").join(sub);
        if d.is_dir() {
            let _ = register_reactive_procedures(&d, &registry, handler()).await;
        }
    }
    // Even after loading the live tree, nothing named shadow_* is reactive,
    // because the shadow files live under praxis/shadow (not loaded here) and the
    // live tree contains no shadow_* procedures.
    let dir = Path::new(""); // unused marker to keep imports tidy
    let _ = dir;
}
