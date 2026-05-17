//! Integration tests for .px file parsing, compilation, and execution.
//!
//! These tests validate the full pipeline: parse → compile → execute
//! using real .px fixture files, including bracket indexing paths.

use pares_radix_praxis::px::parse;
use pares_radix_praxis::px::compiler::compile;
use pares_radix_praxis::px::executor::default_evaluate_condition;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn parse_personality_px() {
    let source = fs::read_to_string(fixtures_dir().join("personality.px")).unwrap();
    let doc = parse(&source).unwrap();

    assert_eq!(doc.constraints.len(), 4);
    assert_eq!(doc.constraints[0].name, "warmth");
    assert_eq!(doc.constraints[1].name, "directness");
    assert_eq!(doc.constraints[2].name, "no_personality_in_code");
    assert_eq!(doc.constraints[3].name, "humor");

    // Verify severity parsing
    assert_eq!(doc.constraints[2].severity, "error");
}

#[test]
fn parse_array_workflow_px() {
    let source = fs::read_to_string(fixtures_dir().join("array_workflow.px")).unwrap();
    let doc = parse(&source).unwrap();

    // Should have facts, constraints, and a procedure
    assert_eq!(doc.facts.len(), 1);
    assert_eq!(doc.facts[0].name, "deployment_targets");

    assert_eq!(doc.constraints.len(), 2);
    assert_eq!(doc.constraints[0].name, "first_env_healthy");
    assert_eq!(doc.constraints[1].name, "prod_replica_minimum");

    assert_eq!(doc.procedures.len(), 1);
    assert_eq!(doc.procedures[0].name, "deploy_check");
}

#[test]
fn parse_server_config_px() {
    let source = fs::read_to_string(fixtures_dir().join("server_config.px")).unwrap();
    let doc = parse(&source).unwrap();

    assert_eq!(doc.facts.len(), 1);
    assert_eq!(doc.facts[0].name, "config");
    assert_eq!(doc.constraints.len(), 1);
    assert_eq!(doc.constraints[0].name, "primary_server_port");
}

#[test]
fn compile_procedures_from_fixture() {
    let source = fs::read_to_string(fixtures_dir().join("array_workflow.px")).unwrap();
    let doc = parse(&source).unwrap();
    assert_eq!(doc.procedures.len(), 1);

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
    assert_eq!(doc.facts.len(), 1);
    assert_eq!(doc.facts[0].name, "pipeline");

    // Constraints with bracket indexing
    assert_eq!(doc.constraints.len(), 2);
    assert_eq!(doc.constraints[0].name, "all_stages_named");
    assert_eq!(doc.constraints[1].name, "prod_stage_requires_approval");
    assert!(doc.constraints[0].require_expr.as_ref().unwrap().contains("[0]"));
    assert!(doc.constraints[1].when_expr.as_ref().unwrap().contains("[2]"));

    // Procedure with when, try/catch, emit
    assert_eq!(doc.procedures.len(), 1);
    assert_eq!(doc.procedures[0].name, "run_pipeline");
    // Steps: call, when, when, try
    assert_eq!(doc.procedures[0].steps.len(), 4);
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
    let proc_record = records.iter().find(|r| {
        r.data.get("name").and_then(|v| v.as_str()) == Some("run_pipeline")
    }).expect("procedure record not found");

    // Execute with pre-seeded health variable
    let vars = HashMap::from([
        ("health".to_string(), json!({"status": "degraded", "uptime": 99.2})),
    ]);
    let result = executor::execute_with_vars(&proc_record.data, &PipelineHandler, vars).unwrap();
    assert!(result.success);

    // Check that emit captured the warning (when condition matched "degraded")
    let emit = result.variables.get("emit");
    assert!(emit.is_some(), "emit should have captured events");
    let emit_arr = emit.unwrap().as_array().unwrap();
    // Should have pipeline_warning + stage_failed events
    assert!(emit_arr.len() >= 1, "should have at least 1 emitted event, got {}", emit_arr.len());

    // Try/catch should have recovered — rollback result captured
    assert!(result.variables.get("rollback").is_some() || result.variables.get("error").is_some(),
        "try/catch should have captured rollback or error");
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
    assert!(default_evaluate_condition("servers[0].host == web-01", &vars));
    assert!(default_evaluate_condition("servers[1].healthy == false", &vars));
    assert!(!default_evaluate_condition("servers[1].healthy", &vars));
    assert!(default_evaluate_condition("servers[0].load < 50", &vars));
    assert!(default_evaluate_condition("servers[1].load > 90", &vars));
    assert!(default_evaluate_condition("servers[2].load < 20 && servers[0].healthy", &vars));
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
    assert!(default_evaluate_condition(
        "!cluster.nodes[2].ready",
        &vars
    ));
}

#[test]
fn parse_parallel_workflow_px() {
    let source = fs::read_to_string(fixtures_dir().join("parallel_workflow.px")).unwrap();
    let doc = parse(&source).unwrap();

    assert_eq!(doc.procedures.len(), 1);
    let proc = &doc.procedures[0];
    assert_eq!(proc.name, "dashboard_refresh");

    // Should have: parallel step followed by emit
    assert_eq!(proc.steps.len(), 2);

    match &proc.steps[0] {
        pares_radix_praxis::px::PxStep::Parallel { branches, output_var } => {
            assert_eq!(branches.len(), 3);
            assert_eq!(branches[0].name, "metrics");
            assert_eq!(branches[1].name, "alerts");
            assert_eq!(branches[2].name, "status");
            // metrics branch has 2 steps
            assert_eq!(branches[0].steps.len(), 2);
            // alerts and status have 1 step each
            assert_eq!(branches[1].steps.len(), 1);
            assert_eq!(branches[2].steps.len(), 1);
            assert_eq!(output_var.as_deref(), Some("data"));
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
