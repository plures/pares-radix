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
