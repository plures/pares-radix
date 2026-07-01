//! Integration tests for .px file parsing, compilation, and execution.
//!
//! These tests validate the full pipeline: parse → compile → execute
//! using real .px fixture files, including bracket indexing paths.

use pares_radix_praxis::px::compiler::compile;
use pares_radix_praxis::px::executor::default_evaluate_condition;
use pares_radix_praxis::px::expr_to_string;
use pares_radix_praxis::px::parse;
use pares_radix_praxis::px::px_ast::{
    ConstraintDecl, FactDecl, ProcedureBody, PxDocument, RuleDecl, Severity, Statement, Step,
};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

// ── AST access helpers (px-ast migration) ─────────────────────────────────────
// The old flat `PxDocument` exposed `.facts`, `.constraints`, `.rules`, and
// `.procedures` as `Vec` fields with `String` names and flat `require_expr` /
// `when_expr` / `steps` fields. px-ast replaces the root with a single
// `statements: Vec<Statement>` enum, decl names are `Ident` (`.name.name`),
// constraint conditions are `Option<Expr>` (`require` / `when`, rendered via
// `expr_to_string`), and a procedure body is a `ProcedureBody` enum. These
// helpers reproduce the old ordered views so each test keeps its exact intent.

fn facts(doc: &PxDocument) -> Vec<&FactDecl> {
    doc.statements
        .iter()
        .filter_map(|s| match s {
            Statement::Fact(f) => Some(f),
            _ => None,
        })
        .collect()
}

fn constraints(doc: &PxDocument) -> Vec<&ConstraintDecl> {
    doc.statements
        .iter()
        .filter_map(|s| match s {
            Statement::Constraint(c) => Some(c),
            _ => None,
        })
        .collect()
}

fn rules(doc: &PxDocument) -> Vec<&RuleDecl> {
    doc.statements
        .iter()
        .filter_map(|s| match s {
            Statement::Rule(r) => Some(r),
            _ => None,
        })
        .collect()
}

/// A procedure view (dataflow + legacy) exposing the fields these tests read:
/// the declared name and the ordered v1 step list. All procedure fixtures here
/// use the legacy `procedure foo:\n  trigger: ...` form, but dataflow decls are
/// included too so the count matches the old summed `doc.procedures` list.
struct ProcView<'a> {
    name: &'a str,
    steps: Vec<&'a Step>,
}

fn procedures(doc: &PxDocument) -> Vec<ProcView<'_>> {
    doc.statements
        .iter()
        .filter_map(|s| match s {
            Statement::LegacyProcedure(p) => Some(ProcView {
                name: p.name.name.as_str(),
                steps: step_list(&p.body),
            }),
            Statement::DataflowProcedure(p) => Some(ProcView {
                name: p.name.name.as_str(),
                steps: step_list(&p.body),
            }),
            _ => None,
        })
        .collect()
}

/// Extract the v1 step list from a procedure body. Code-block (v2) bodies have
/// no `Step` list; the fixtures in this suite are all v1 step lists, so an empty
/// slice is the honest answer for a code body (and would fail a step-count
/// assertion loudly rather than silently masking a shape change).
fn step_list(body: &ProcedureBody) -> Vec<&Step> {
    match body {
        ProcedureBody::Steps(steps) => steps.iter().collect(),
        ProcedureBody::Code(_) => Vec::new(),
    }
}

#[test]
fn parse_personality_px() {
    let source = fs::read_to_string(fixtures_dir().join("personality.px")).unwrap();
    let doc = parse(&source).unwrap();

    let cs = constraints(&doc);
    assert_eq!(cs.len(), 4);
    assert_eq!(cs[0].name.name, "warmth");
    assert_eq!(cs[1].name.name, "directness");
    assert_eq!(cs[2].name.name, "no_personality_in_code");
    assert_eq!(cs[3].name.name, "humor");

    // Verify severity parsing
    assert_eq!(cs[2].severity, Severity::Error);
}

#[test]
fn parse_array_workflow_px() {
    let source = fs::read_to_string(fixtures_dir().join("array_workflow.px")).unwrap();
    let doc = parse(&source).unwrap();

    // Should have facts, constraints, and a procedure
    let fs_ = facts(&doc);
    assert_eq!(fs_.len(), 1);
    assert_eq!(fs_[0].name.name, "deployment_targets");

    let cs = constraints(&doc);
    assert_eq!(cs.len(), 2);
    assert_eq!(cs[0].name.name, "first_env_healthy");
    assert_eq!(cs[1].name.name, "prod_replica_minimum");

    let ps = procedures(&doc);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].name, "deploy_check");
}

#[test]
fn parse_server_config_px() {
    let source = fs::read_to_string(fixtures_dir().join("server_config.px")).unwrap();
    let doc = parse(&source).unwrap();

    let fs_ = facts(&doc);
    assert_eq!(fs_.len(), 1);
    assert_eq!(fs_[0].name.name, "config");
    let cs = constraints(&doc);
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].name.name, "primary_server_port");
}

#[test]
fn compile_procedures_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("array_workflow.px")).unwrap();
    let doc = parse(&source).unwrap();
    assert_eq!(procedures(&doc).len(), 1);

    let compiled = compile(&doc);
    // compile returns CompiledRecord list — should include the procedure
    assert!(!compiled.is_empty());
}

#[test]
fn bracket_indexing_in_condition_evaluation_with_real_data() {
    // Simulate data that would come from a .px fact evaluation
    let vars = HashMap::from([
        (
            "deployment_targets".to_string(),
            json!({
                "environments": [
                    {"name": "dev", "healthy": true, "replicas": 3},
                    {"name": "staging", "healthy": true, "replicas": 2},
                    {"name": "production", "healthy": false, "replicas": 5}
                ]
            }),
        ),
        (
            "config".to_string(),
            json!({
                "servers": [
                    {"host": "web-01", "port": 8080, "tags": ["frontend", "primary"]},
                    {"host": "web-02", "port": 8081, "tags": ["frontend", "secondary"]},
                    {"host": "api-01", "port": 3000, "tags": ["backend", "primary"]}
                ]
            }),
        ),
    ]);

    // Array index + nested object
    assert!(default_evaluate_condition(
        "deployment_targets.environments[0].name == dev",
        &vars
    ));
    assert!(default_evaluate_condition(
        "deployment_targets.environments[2].name == production",
        &vars
    ));
    assert!(default_evaluate_condition(
        "deployment_targets.environments[0].healthy",
        &vars
    ));
    assert!(!default_evaluate_condition(
        "deployment_targets.environments[2].healthy",
        &vars
    ));

    // Numeric comparison with bracket indexing
    assert!(default_evaluate_condition(
        "deployment_targets.environments[2].replicas >= 3",
        &vars
    ));
    assert!(default_evaluate_condition(
        "config.servers[0].port == 8080",
        &vars
    ));
    assert!(default_evaluate_condition(
        "config.servers[2].port < 5000",
        &vars
    ));

    // String comparison with bracket indexing
    assert!(default_evaluate_condition(
        "config.servers[0].host == web-01",
        &vars
    ));
    assert!(!default_evaluate_condition(
        "config.servers[0].host == web-02",
        &vars
    ));
}

#[test]
fn parse_pipeline_workflow_px() {
    let source = fs::read_to_string(fixtures_dir().join("pipeline_workflow.px")).unwrap();
    let doc = parse(&source).unwrap();

    // Facts
    let fs_ = facts(&doc);
    assert_eq!(fs_.len(), 1);
    assert_eq!(fs_[0].name.name, "pipeline");

    // Constraints with bracket indexing
    let cs = constraints(&doc);
    assert_eq!(cs.len(), 2);
    assert_eq!(cs[0].name.name, "all_stages_named");
    assert_eq!(cs[1].name.name, "prod_stage_requires_approval");
    assert!(expr_to_string(cs[0].require.as_ref().unwrap()).contains("[0]"));
    assert!(expr_to_string(cs[1].when.as_ref().unwrap()).contains("[2]"));

    // Procedure with when, try/catch, emit
    let ps = procedures(&doc);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].name, "run_pipeline");
    // Steps: call, when, when, try
    assert_eq!(ps[0].steps.len(), 4);
}

#[test]
fn compile_and_execute_pipeline_workflow() {
    use pares_radix_praxis::px::compiler::compile;
    use pares_radix_praxis::px::executor::{self, ActionHandler, ExecutionError};
    use serde_json::Value;

    struct PipelineHandler;
    impl ActionHandler for PipelineHandler {
        fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            match name {
                "check_stage_health" => Ok(json!({"status": "degraded", "uptime": 99.2})),
                "deploy_stage" => Err(ExecutionError::ActionFailed {
                    action: "deploy_stage".into(),
                    message: "timeout".into(),
                }),
                "rollback_stage" => Ok(json!("rolled_back")),
                _ => Err(ExecutionError::UnknownAction(name.to_string())),
            }
        }
    }

    let source = fs::read_to_string(fixtures_dir().join("pipeline_workflow.px")).unwrap();
    let doc = parse(&source).unwrap();
    let records = compile(&doc);

    // Find the procedure record
    let proc_record = records
        .iter()
        .find(|r| r.data.get("name").and_then(|v| v.as_str()) == Some("run_pipeline"))
        .expect("procedure record not found");

    // Execute with pre-seeded health variable
    let vars = HashMap::from([(
        "health".to_string(),
        json!({"status": "degraded", "uptime": 99.2}),
    )]);
    let result = executor::execute_with_vars(&proc_record.data, &PipelineHandler, vars).unwrap();
    assert!(result.success);

    // Check that emit captured the warning (when condition matched "degraded")
    let emit = result.variables.get("emit");
    assert!(emit.is_some(), "emit should have captured events");
    let emit_arr = emit.unwrap().as_array().unwrap();
    // Should have pipeline_warning + stage_failed events
    assert!(
        !emit_arr.is_empty(),
        "should have at least 1 emitted event, got {}",
        emit_arr.len()
    );

    // Try/catch should have recovered — rollback result captured
    assert!(
        result.variables.contains_key("rollback") || result.variables.contains_key("error"),
        "try/catch should have captured rollback or error"
    );
}

#[test]
fn loop_with_bracket_indexed_conditions() {
    // Test: loop produces results, then bracket-index those results in conditions
    use pares_radix_praxis::px::executor::{self, ActionHandler, ExecutionError};
    use serde_json::Value;

    struct LoopHandler;
    impl ActionHandler for LoopHandler {
        fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
            match name {
                "fetch_servers" => Ok(json!([
                    {"host": "web-01", "healthy": true, "load": 42.5},
                    {"host": "web-02", "healthy": false, "load": 98.1},
                    {"host": "web-03", "healthy": true, "load": 15.0}
                ])),
                "check_server" => {
                    // Just echo back what was passed
                    Ok(params.clone())
                }
                _ => Err(ExecutionError::UnknownAction(name.to_string())),
            }
        }
    }

    // Build a procedure JSON directly (simulating compiled output)
    let procedure = json!({
        "name": "server_health_check",
        "steps": [
            { "kind": "call", "name": "fetch_servers", "params": {}, "output_var": "servers" },
            {
                "kind": "loop",
                "over": "servers",
                "as": "server",
                "steps": [
                    { "kind": "call", "name": "check_server", "params": { "host": "$server" } }
                ],
                "output_var": "check_results"
            }
        ]
    });

    let result = executor::execute(&procedure, &LoopHandler).unwrap();
    assert!(result.success);

    // Verify servers were fetched
    let servers = result.variables.get("servers").unwrap();
    assert_eq!(servers.as_array().unwrap().len(), 3);

    // Verify loop produced results
    let check_results = result.variables.get("check_results").unwrap();
    assert_eq!(check_results.as_array().unwrap().len(), 3);

    // Now verify bracket indexing works on the loop output
    let vars = result.variables.clone();
    use pares_radix_praxis::px::executor::default_evaluate_condition;
    assert!(default_evaluate_condition(
        "servers[0].host == web-01",
        &vars
    ));
    assert!(default_evaluate_condition(
        "servers[1].healthy == false",
        &vars
    ));
    assert!(!default_evaluate_condition("servers[1].healthy", &vars));
    assert!(default_evaluate_condition("servers[0].load < 50", &vars));
    assert!(default_evaluate_condition("servers[1].load > 90", &vars));
    assert!(default_evaluate_condition(
        "servers[2].load < 20 && servers[0].healthy",
        &vars
    ));
}

#[test]
fn nested_try_in_loop() {
    // Test: try/catch inside a loop — errors in one iteration don't stop others
    use pares_radix_praxis::px::executor::{self, ActionHandler, ExecutionError};
    use serde_json::Value;

    struct FlakyHandler {
        fail_on: Vec<String>,
    }
    impl ActionHandler for FlakyHandler {
        fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
            match name {
                "process" => {
                    let val = params.get("val").and_then(|v| v.as_str()).unwrap_or("");
                    if self.fail_on.contains(&val.to_string()) {
                        Err(ExecutionError::ActionFailed {
                            action: "process".into(),
                            message: format!("failed on {}", val),
                        })
                    } else {
                        Ok(json!(format!("ok:{}", val)))
                    }
                }
                "recover" => Ok(json!("recovered")),
                _ => Err(ExecutionError::UnknownAction(name.to_string())),
            }
        }
    }

    let procedure = json!({
        "name": "resilient_batch",
        "steps": [
            {
                "kind": "loop",
                "over": "items",
                "as": "item",
                "steps": [
                    {
                        "kind": "try",
                        "steps": [
                            { "kind": "call", "name": "process", "params": { "val": "$item" } }
                        ],
                        "catch": [
                            { "kind": "call", "name": "recover", "params": {} }
                        ]
                    }
                ],
                "output_var": "results"
            }
        ]
    });

    let handler = FlakyHandler {
        fail_on: vec!["b".to_string()],
    };
    let vars = HashMap::from([("items".to_string(), json!(["a", "b", "c"]))]);

    let result = executor::execute_with_vars(&procedure, &handler, vars).unwrap();
    assert!(result.success);

    // All 3 iterations should complete (try/catch prevents propagation)
    let results = result.variables.get("results").unwrap().as_array().unwrap();
    assert_eq!(results.len(), 3);
    // First and third succeeded via process, second recovered via catch
    // The try step output is the catch handler's output for failed items
    assert_eq!(results[1], json!("recovered"));
}

#[test]
fn emit_with_loop_variable_resolution() {
    // Test: emit inside a loop resolves $item correctly
    use pares_radix_praxis::px::executor::{self, ActionHandler, ExecutionError};
    use serde_json::Value;

    struct NoopHandler;
    impl ActionHandler for NoopHandler {
        fn call(&self, _name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            Ok(Value::Null)
        }
    }

    let procedure = json!({
        "name": "notify_all",
        "steps": [
            {
                "kind": "loop",
                "over": "users",
                "as": "user",
                "steps": [
                    {
                        "kind": "emit",
                        "event": { "type": "notification", "target": "$user" }
                    }
                ]
            }
        ]
    });

    let vars = HashMap::from([("users".to_string(), json!(["alice", "bob", "charlie"]))]);
    let result = executor::execute_with_vars(&procedure, &NoopHandler, vars).unwrap();
    assert!(result.success);

    let emit = result.variables.get("emit").unwrap().as_array().unwrap();
    assert_eq!(emit.len(), 3);
    assert_eq!(emit[0]["target"], "alice");
    assert_eq!(emit[1]["target"], "bob");
    assert_eq!(emit[2]["target"], "charlie");
}

#[test]
fn bracket_indexing_in_logical_combinations() {
    let vars = HashMap::from([(
        "cluster".to_string(),
        json!({
            "nodes": [
                {"name": "node-1", "ready": true, "cpu": 45.2},
                {"name": "node-2", "ready": true, "cpu": 82.1},
                {"name": "node-3", "ready": false, "cpu": 0.0}
            ]
        }),
    )]);

    // AND with bracket indexing
    assert!(default_evaluate_condition(
        "cluster.nodes[0].ready && cluster.nodes[1].ready",
        &vars
    ));
    assert!(!default_evaluate_condition(
        "cluster.nodes[0].ready && cluster.nodes[2].ready",
        &vars
    ));

    // OR with bracket indexing
    assert!(default_evaluate_condition(
        "cluster.nodes[2].ready || cluster.nodes[0].ready",
        &vars
    ));

    // Comparison + logical with bracket indexing
    assert!(default_evaluate_condition(
        "cluster.nodes[1].cpu > 80 && cluster.nodes[0].cpu < 50",
        &vars
    ));

    // Negation with bracket indexing
    assert!(default_evaluate_condition("!cluster.nodes[2].ready", &vars));
}

#[test]
fn parse_parallel_workflow_px() {
    let source = fs::read_to_string(fixtures_dir().join("parallel_workflow.px")).unwrap();
    let doc = parse(&source).unwrap();

    let ps = procedures(&doc);
    assert_eq!(ps.len(), 1);
    let proc = &ps[0];
    assert_eq!(proc.name, "dashboard_refresh");

    // Should have: parallel step followed by emit
    assert_eq!(proc.steps.len(), 2);

    match proc.steps[0] {
        Step::Parallel(parallel) => {
            let branches = &parallel.branches;
            assert_eq!(branches.len(), 3);
            assert_eq!(branches[0].name.name, "metrics");
            assert_eq!(branches[1].name.name, "alerts");
            assert_eq!(branches[2].name.name, "status");
            // metrics branch has 2 steps
            assert_eq!(branches[0].steps.len(), 2);
            // alerts and status have 1 step each
            assert_eq!(branches[1].steps.len(), 1);
            assert_eq!(branches[2].steps.len(), 1);
            assert_eq!(
                parallel.output.as_ref().map(|i| i.name.as_str()),
                Some("data")
            );
        }
        other => panic!("expected Parallel step, got {:?}", other),
    }
}

#[test]
fn compile_and_execute_parallel_workflow() {
    use pares_radix_praxis::px::executor::{self, ActionHandler, ExecutionError};
    use serde_json::Value;

    struct DashHandler;
    impl ActionHandler for DashHandler {
        fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
            match name {
                "fetch_metrics" => Ok(json!({"cpu": 42, "mem": 80})),
                "transform_metrics" => Ok(json!({"cpu_pct": "42%", "mem_pct": "80%"})),
                "fetch_alerts" => Ok(json!([{"id": 1, "msg": "disk low"}])),
                "check_health" => Ok(json!("healthy")),
                _ => Err(ExecutionError::UnknownAction(name.to_string())),
            }
        }
    }

    let source = fs::read_to_string(fixtures_dir().join("parallel_workflow.px")).unwrap();
    let doc = parse(&source).unwrap();
    let records = compile(&doc);
    assert_eq!(records.len(), 1);

    let result = executor::execute(&records[0].data, &DashHandler).unwrap();
    assert!(result.success);

    // Check parallel output is bound
    let data = result.variables.get("data").unwrap();
    assert_eq!(data["metrics"], json!({"cpu_pct": "42%", "mem_pct": "80%"}));
    assert_eq!(data["alerts"], json!([{"id": 1, "msg": "disk low"}]));
    assert_eq!(data["status"], json!("healthy"));

    // Check emit captured the data
    let emit = result.variables.get("emit").unwrap().as_array().unwrap();
    assert_eq!(emit.len(), 1);
    assert_eq!(emit[0]["type"], "dashboard_updated");
}

// ─── Example Files Validation ─────────────────────────────────────────────────

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/px")
}

#[test]
fn parse_example_ci_pipeline() {
    let source = fs::read_to_string(examples_dir().join("ci-pipeline.px")).unwrap();
    let doc = parse(&source).unwrap();

    let fs_ = facts(&doc);
    assert_eq!(fs_.len(), 1);
    assert_eq!(fs_[0].name.name, "BuildConfig");
    assert_eq!(constraints(&doc).len(), 1);
    let ps = procedures(&doc);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].name, "ci_pipeline");

    let compiled = compile(&doc);
    assert!(!compiled.is_empty());
}

#[test]
fn parse_example_incident_response() {
    let source = fs::read_to_string(examples_dir().join("incident-response.px")).unwrap();
    let doc = parse(&source).unwrap();

    assert_eq!(facts(&doc).len(), 2);
    assert_eq!(rules(&doc).len(), 1);
    assert_eq!(procedures(&doc).len(), 4); // respond + handle_p1..p3

    let compiled = compile(&doc);
    assert!(!compiled.is_empty());
}

#[test]
fn parse_example_data_sync() {
    let source = fs::read_to_string(examples_dir().join("data-sync.px")).unwrap();
    let doc = parse(&source).unwrap();

    assert_eq!(facts(&doc).len(), 2);
    assert_eq!(procedures(&doc).len(), 2); // nightly_data_sync + resync_source

    let compiled = compile(&doc);
    assert!(!compiled.is_empty());
}

#[test]
fn parse_example_pr_review_bot() {
    let source = fs::read_to_string(examples_dir().join("pr-review-bot.px")).unwrap();
    let doc = parse(&source).unwrap();

    assert_eq!(facts(&doc).len(), 2);
    assert_eq!(constraints(&doc).len(), 2);
    assert_eq!(rules(&doc).len(), 3);
    let ps = procedures(&doc);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].name, "review_pr");

    let compiled = compile(&doc);
    assert!(!compiled.is_empty());
}

#[test]
fn parse_example_memory_maintenance() {
    let source = fs::read_to_string(examples_dir().join("memory-maintenance.px")).unwrap();
    let doc = parse(&source).unwrap();

    assert_eq!(facts(&doc).len(), 1);
    assert_eq!(procedures(&doc).len(), 2); // memory_maintenance + cleanup_category

    let compiled = compile(&doc);
    assert!(!compiled.is_empty());
}
