//! Integration tests for .px compose + resolver working together.
//!
//! Tests the full pipeline of: parse → resolve imports → build registry → compose execution.
//! Validates that multi-file .px projects work end-to-end.

use async_trait::async_trait;
use pares_radix_praxis::px::compose::{pipe, ComposableHandler, ProcedureRegistry};
use pares_radix_praxis::px::async_executor::{execute_async_with_vars, AsyncActionHandler};
use pares_radix_praxis::px::executor::ExecutionError;
use pares_radix_praxis::px::resolver::{resolve_imports, ResolveError};
use pares_radix_praxis::px::parse;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

// ── Test Handler ──────────────────────────────────────────────────────────────

/// A configurable handler for integration tests that tracks calls.
struct TestHandler {
    results: HashMap<String, Value>,
    calls: std::sync::Mutex<Vec<(String, Value)>>,
}

impl TestHandler {
    fn new() -> Self {
        Self {
            results: HashMap::new(),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn with_result(mut self, name: &str, value: Value) -> Self {
        self.results.insert(name.to_string(), value);
        self
    }

    fn call_log(&self) -> Vec<(String, Value)> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl AsyncActionHandler for TestHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        self.calls.lock().unwrap().push((name.to_string(), params.clone()));
        self.results
            .get(name)
            .cloned()
            .ok_or_else(|| ExecutionError::UnknownAction(name.to_string()))
    }
}

// ── Resolver Tests ────────────────────────────────────────────────────────────

#[test]
fn resolver_resolves_single_import() {
    let source = fs::read_to_string(fixtures_dir().join("pipeline_composed.px")).unwrap();
    let doc = parse(&source).unwrap();

    // Should have imports before resolution
    assert_eq!(doc.imports.len(), 2);

    let resolved = resolve_imports(&doc, &fixtures_dir()).unwrap();

    // After resolution, imports are inlined
    assert!(resolved.document.imports.is_empty());

    // Procedures from validation_utils and math_utils should be merged in
    let proc_names: Vec<&str> = resolved.document.procedures.iter().map(|p| p.name.as_str()).collect();
    assert!(proc_names.contains(&"validate_input"), "missing validate_input, got: {:?}", proc_names);
    assert!(proc_names.contains(&"sanitize"), "missing sanitize, got: {:?}", proc_names);
    assert!(proc_names.contains(&"double"), "missing double, got: {:?}", proc_names);
    assert!(proc_names.contains(&"process_request"), "missing process_request, got: {:?}", proc_names);

    // The local constraint should still be there
    assert!(resolved.document.constraints.iter().any(|c| c.name == "timeout_sane"));
}

#[test]
fn resolver_detects_circular_imports() {
    let source = fs::read_to_string(fixtures_dir().join("circular_a.px")).unwrap();
    let doc = parse(&source).unwrap();

    let result = resolve_imports(&doc, &fixtures_dir());
    assert!(result.is_err());

    match result.unwrap_err() {
        ResolveError::CircularImport { .. } => {} // expected
        other => panic!("expected CircularImport, got: {:?}", other),
    }
}

#[test]
fn resolver_diamond_import_dedup() {
    // Both validation_utils and math_utils are independent — if a third file
    // imports both, they should only appear once each (no duplicates).
    let source = r#"
import validation_utils
import math_utils
import validation_utils

procedure main_proc:
  trigger: manual
  do_thing {} -> $x
"#;
    let doc = parse(source).unwrap();
    let resolved = resolve_imports(&doc, &fixtures_dir()).unwrap();

    // Count how many times "validate_input" appears — should be exactly 1
    let count = resolved.document.procedures.iter()
        .filter(|p| p.name == "validate_input")
        .count();
    assert_eq!(count, 1, "diamond dedup should prevent duplicate procedures");
}

#[test]
fn resolver_parse_error_in_import_reports_path() {
    // Create a temp file with invalid .px syntax
    let tmp_dir = tempfile::tempdir().unwrap();
    let bad_file = tmp_dir.path().join("bad_syntax.px");
    fs::write(&bad_file, "this is not valid px syntax @@@@").unwrap();

    let source = "import bad_syntax\n\nprocedure main:\n  trigger: manual\n  noop {} -> $x\n";
    let doc = parse(source).unwrap();
    let result = resolve_imports(&doc, tmp_dir.path());

    // Should either be IoError or ParseError with the bad file path
    match result {
        Err(ResolveError::ParseError { path, .. }) => {
            assert!(path.ends_with("bad_syntax.px"));
        }
        Err(ResolveError::IoError { path, .. }) => {
            assert!(path.ends_with("bad_syntax.px"));
        }
        Ok(_) => panic!("expected error for invalid .px file"),
        Err(other) => panic!("unexpected error variant: {:?}", other),
    }
}

// ── Compose + Resolver Integration ───────────────────────────────────────────

#[tokio::test]
async fn compose_resolved_procedures_execute() {
    // Parse and resolve the composed pipeline
    let source = fs::read_to_string(fixtures_dir().join("pipeline_composed.px")).unwrap();
    let doc = parse(&source).unwrap();
    let resolved = resolve_imports(&doc, &fixtures_dir()).unwrap();

    // Verify all procedures from imports + local are present
    let proc_names: Vec<&str> = resolved.document.procedures.iter()
        .map(|p| p.name.as_str())
        .collect();

    assert!(proc_names.contains(&"validate_input"), "got: {:?}", proc_names);
    assert!(proc_names.contains(&"sanitize"), "got: {:?}", proc_names);
    assert!(proc_names.contains(&"double"), "got: {:?}", proc_names);
    assert!(proc_names.contains(&"process_request"), "got: {:?}", proc_names);

    // Build a registry — use procedure name as key, full serialized proc as value
    let mut registry = ProcedureRegistry::new();
    for proc_decl in &resolved.document.procedures {
        let proc_data = serde_json::to_value(proc_decl).unwrap();
        registry.register_as(&proc_decl.name, proc_data);
    }

    // All imported procedures should be in the registry
    assert!(registry.contains("validate_input"));
    assert!(registry.contains("sanitize"));
    assert!(registry.contains("double"));
    assert!(registry.contains("process_request"));
    assert_eq!(registry.len(), 4);
}

#[tokio::test]
async fn compose_simple_pipe_with_resolved_procs() {
    // Create two procedures that form a pipeline
    let proc_a = json!({
        "type": "procedure",
        "name": "step_one",
        "steps": [
            { "kind": "call", "name": "fetch_data", "params": {}, "output_var": "output" }
        ]
    });

    let proc_b = json!({
        "type": "procedure",
        "name": "step_two",
        "steps": [
            { "kind": "call", "name": "process_data", "params": {"data": "$input"}, "output_var": "output" }
        ]
    });

    let mut registry = ProcedureRegistry::new();
    registry.register(proc_a);
    registry.register(proc_b);

    let handler = TestHandler::new()
        .with_result("fetch_data", json!({"items": [1, 2, 3]}))
        .with_result("process_data", json!("processed"));

    let result = pipe(&["step_one", "step_two"], &registry, &handler, json!(null))
        .await
        .unwrap();

    assert_eq!(result, json!("processed"));

    // Verify both leaf actions were called
    let calls = handler.call_log();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, "fetch_data");
    assert_eq!(calls[1].0, "process_data");
}

#[tokio::test]
async fn compose_nested_procedure_calls_thread_variables() {
    // outer calls inner, inner calls leaf
    let inner = json!({
        "type": "procedure",
        "name": "inner",
        "steps": [
            { "kind": "call", "name": "leaf_op", "params": {"x": "$value"}, "output_var": "output" }
        ]
    });

    let outer = json!({
        "type": "procedure",
        "name": "outer",
        "steps": [
            { "kind": "call", "name": "inner", "params": {"value": "hello"}, "output_var": "result" }
        ]
    });

    let mut registry = ProcedureRegistry::new();
    registry.register(inner);

    let handler = TestHandler::new()
        .with_result("leaf_op", json!("leaf_output"));

    let composable = ComposableHandler::new(registry, handler);

    let result = execute_async_with_vars(&outer, &composable, HashMap::new())
        .await
        .unwrap();

    assert!(result.success);
    let call_result = result.variables.get("result").unwrap();
    assert_eq!(call_result.get("output"), Some(&json!("leaf_output")));
}

#[tokio::test]
async fn compose_pipe_missing_stage_gives_clear_error() {
    let proc_a = json!({
        "type": "procedure",
        "name": "exists",
        "steps": [
            { "kind": "call", "name": "action", "params": {}, "output_var": "output" }
        ]
    });

    let mut registry = ProcedureRegistry::new();
    registry.register(proc_a);

    let handler = TestHandler::new().with_result("action", json!("ok"));

    let result = pipe(&["exists", "does_not_exist"], &registry, &handler, json!(null)).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("does_not_exist"), "error should name the missing procedure: {}", err_msg);
    assert!(err_msg.contains("not found"), "error should say 'not found': {}", err_msg);
}

#[tokio::test]
async fn compose_recursive_depth_limit_integration() {
    // Create a procedure that calls itself — should hit depth limit
    let recursive = json!({
        "type": "procedure",
        "name": "loop_forever",
        "steps": [
            { "kind": "call", "name": "loop_forever", "params": {}, "output_var": "x" }
        ]
    });

    let mut registry = ProcedureRegistry::new();
    registry.register(recursive.clone());

    let handler = TestHandler::new();
    let composable = ComposableHandler::new(registry, handler);

    let result = execute_async_with_vars(&recursive, &composable, HashMap::new()).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("exceeds maximum") || err_msg.contains("depth"),
        "error should mention depth limit: {}", err_msg);
}

#[tokio::test]
async fn compose_empty_registry_all_calls_fall_through() {
    let proc = json!({
        "type": "procedure",
        "name": "main",
        "steps": [
            { "kind": "call", "name": "action_a", "params": {}, "output_var": "a" },
            { "kind": "call", "name": "action_b", "params": {"from": "$a"}, "output_var": "b" }
        ]
    });

    let registry = ProcedureRegistry::new(); // empty
    let handler = TestHandler::new()
        .with_result("action_a", json!(42))
        .with_result("action_b", json!("done"));

    let composable = ComposableHandler::new(registry, handler);
    let result = execute_async_with_vars(&proc, &composable, HashMap::new())
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.variables.get("a"), Some(&json!(42)));
    assert_eq!(result.variables.get("b"), Some(&json!("done")));
}
