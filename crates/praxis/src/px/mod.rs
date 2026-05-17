//! Praxis Intent Language (.px) parser.
//!
//! Parses `.px` files into typed AST nodes using the pest PEG grammar.

pub mod async_executor;
pub mod builder;
pub mod compiler;
pub mod compose;
pub mod executor;
pub mod resolver;
pub mod watcher;

use pest::Parser;
use pest_derive::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[grammar = "px/grammar.pest"]
pub struct PxParser;

/// A parsed .px document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxDocument {
    pub imports: Vec<PxImport>,
    pub facts: Vec<PxFact>,
    pub rules: Vec<PxRule>,
    pub constraints: Vec<PxConstraint>,
    pub contracts: Vec<PxContract>,
    pub functions: Vec<PxFunction>,
    pub triggers: Vec<PxTrigger>,
    pub procedures: Vec<PxProcedure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxImport {
    pub path: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxFact {
    pub name: String,
    pub fields: Vec<PxField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxField {
    pub name: String,
    pub type_expr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxRule {
    pub name: String,
    pub priority: Option<i32>,
    pub conditions: Vec<String>,
    pub lets: Vec<(String, String)>,
    pub actions: Vec<PxAction>,
    pub captures: Vec<PxCapture>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxAction {
    pub kind: String,
    pub params: std::collections::HashMap<String, serde_json::Value>,
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxCapture {
    pub content: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxConstraint {
    pub name: String,
    pub scope: Option<String>,
    pub phases: Vec<String>,
    pub trait_category: Option<String>,
    pub weight: Option<f64>,
    pub prompt_injection: Option<String>,
    pub when_expr: Option<String>,
    pub require_expr: Option<String>,
    pub severity: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxContract {
    pub name: String,
    pub given: Option<String>,
    pub when_desc: Option<String>,
    pub then_desc: Option<String>,
    pub threshold: Option<f64>,
    pub examples: Vec<PxExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxExample {
    pub input: serde_json::Value,
    pub expect: serde_json::Value,
    pub threshold: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxFunction {
    pub name: String,
    pub params: Vec<PxField>,
    pub return_type: String,
    pub mode: FunctionMode,
    pub docstring: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum FunctionMode {
    #[default]
    Deterministic,
    Probabilistic,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxTrigger {
    pub name: String,
    pub on_event: String,
    pub schedule: Option<String>,
    pub run: String,
}

/// A procedure — a sequence of steps triggered by events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxProcedure {
    pub name: String,
    pub trigger: Option<PxProcedureTrigger>,
    pub given: Option<String>,
    pub steps: Vec<PxStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxProcedureTrigger {
    pub kind: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PxStep {
    Call {
        name: String,
        params: serde_json::Value,
        output_var: Option<String>,
    },
    Match {
        arms: Vec<PxMatchArm>,
    },
    When {
        condition: String,
        steps: Vec<PxStep>,
    },
    Loop {
        /// Variable name to iterate over (resolved from vars at runtime).
        over: Option<String>,
        /// Fixed number of iterations (alternative to `over`).
        times: Option<u64>,
        /// Variable name for the current item (default: "item").
        item_var: String,
        /// Nested steps executed per iteration.
        steps: Vec<PxStep>,
        /// Optional variable to collect results into.
        output_var: Option<String>,
    },
    Emit {
        /// Event data to emit (may contain $variable references).
        event: serde_json::Value,
    },
    Try {
        /// Steps to attempt.
        steps: Vec<PxStep>,
        /// Steps to execute on error.
        catch: Vec<PxStep>,
    },
    Parallel {
        /// Named branches to execute concurrently.
        branches: Vec<PxParallelBranch>,
        /// Optional variable to collect results into (map of branch_name → last output).
        output_var: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxMatchArm {
    pub condition: String,
    pub result: String,
}

/// A named branch within a parallel step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxParallelBranch {
    /// Branch name (used as key in output map).
    pub name: String,
    /// Steps to execute within this branch.
    pub steps: Vec<PxStep>,
}

/// Parse a .px source string into a document AST.
pub fn parse(source: &str) -> Result<PxDocument, String> {
    let pairs = PxParser::parse(Rule::document, source).map_err(|e| format!("parse error: {e}"))?;

    Ok(builder::build(pairs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_compiles() {
        let _ = PxParser::parse(Rule::ident, "hello");
    }

    #[test]
    fn parse_simple_fact() {
        let result = PxParser::parse(Rule::ident, "pr_state");
        assert!(result.is_ok());
    }

    #[test]
    fn parse_constraint_expr() {
        let result = PxParser::parse(Rule::expr, "pr.ci_status == green");
        assert!(result.is_ok(), "failed to parse expression");
    }

    #[test]
    fn parse_expr_with_symbolic_logic_ops() {
        // && and || should work alongside 'and' and 'or'
        let cases = [
            "a == b && c == d",
            "a == b || c == d",
            "x > 1 && y < 10 || z == 0",
            "a and b",
            "a or b",
        ];
        for case in cases {
            let result = PxParser::parse(Rule::expr, case);
            assert!(result.is_ok(), "failed to parse expr: {case}");
        }
    }

    #[test]
    fn parse_value_types() {
        assert!(PxParser::parse(Rule::value, "\"hello\"").is_ok());
        assert!(PxParser::parse(Rule::value, "42").is_ok());
        assert!(PxParser::parse(Rule::value, "3.14").is_ok());
        assert!(PxParser::parse(Rule::value, "true").is_ok());
        assert!(PxParser::parse(Rule::value, "false").is_ok());
    }

    #[test]
    fn parse_document_extracts_rule_constraint_and_contract() {
        let source = r#"
fact pr_state:
  ci_status: enum(green, failing, pending)
  has_review: bool

rule auto_merge:
  when:
    - pr_state.ci_status == green
    - pr_state.has_review == true
  then:
    - action: merge_pr method: "squash"
  capture:
    - fact: "Merged PR" category: work_in_progress tags: ["lifecycle", "merge"]

constraint merge_requires_review:
  when: pr_state.ci_status == green
  require: pr_state.has_review == true
  severity: error

contract auto_merge_behavior:
  given: "CI green + reviewed"
  when: "lifecycle evaluates"
  then: "PR merged"
  examples:
    - input: { ci_status: green, has_review: true }
      expect: [merge_pr]
"#;

        let document = parse(source).expect("expected valid .px document");
        assert_eq!(document.facts.len(), 1);
        assert_eq!(document.rules.len(), 1);
        assert_eq!(document.constraints.len(), 1);
        assert_eq!(document.contracts.len(), 1);

        let rule = &document.rules[0];
        assert_eq!(rule.conditions.len(), 2);
        assert_eq!(rule.actions.len(), 1);
        assert_eq!(rule.captures.len(), 1);
        assert_eq!(rule.captures[0].content, "Merged PR");
        assert_eq!(
            rule.captures[0].category.as_deref(),
            Some("work_in_progress")
        );
        assert_eq!(rule.captures[0].tags, vec!["lifecycle", "merge"]);
    }

    #[test]
    fn parse_personality_constraint() {
        let source = r#"
constraint warmth:
  phase: user_communication, error_reporting
  trait: warmth
  weight: 0.8
  prompt: "Use a warm, approachable tone."
  severity: info
"#;

        let doc = parse(source).expect("expected valid .px");
        assert_eq!(doc.constraints.len(), 1);
        let c = &doc.constraints[0];
        assert_eq!(c.name, "warmth");
        assert_eq!(c.trait_category.as_deref(), Some("warmth"));
        assert_eq!(c.phases, vec!["user_communication", "error_reporting"]);
        assert!((c.weight.unwrap() - 0.8).abs() < f64::EPSILON);
        assert_eq!(
            c.prompt_injection.as_deref(),
            Some("Use a warm, approachable tone.")
        );
        assert_eq!(c.severity, "info");
        // when/require are optional for personality constraints
        assert!(c.when_expr.is_none());
        assert!(c.require_expr.is_none());
    }

    #[test]
    fn parse_constraint_with_symbolic_operators() {
        let source = r#"
constraint deploy_gate:
  when: ci.status == green && review.approved == true
  require: deploy.target != production || deploy.canary_ok == true
  severity: error
  message: "Cannot deploy to production without canary pass"
"#;

        let doc = parse(source).expect("expected valid .px");
        assert_eq!(doc.constraints.len(), 1);
        let c = &doc.constraints[0];
        assert_eq!(c.name, "deploy_gate");
        assert!(c.when_expr.as_ref().unwrap().contains("&&"));
        assert!(c.require_expr.as_ref().unwrap().contains("||"));
        assert_eq!(c.severity, "error");
    }

    #[test]
    fn parse_procedure_with_loop() {
        let source = "procedure batch_process:\n  trigger: manual\n  fetch_items {} -> $items\n  loop over $items as item -> $results:\n    process_item {val: $item}\n  end\n";

        let doc = parse(source).expect("parse failed");
        assert_eq!(doc.procedures.len(), 1);
        let proc = &doc.procedures[0];
        assert_eq!(proc.name, "batch_process");
        assert_eq!(proc.steps.len(), 2);

        match &proc.steps[1] {
            PxStep::Loop {
                over,
                times,
                item_var,
                steps,
                output_var,
            } => {
                assert_eq!(over.as_deref(), Some("$items"));
                assert!(times.is_none());
                assert_eq!(item_var, "item");
                assert_eq!(output_var.as_deref(), Some("results"));
                assert_eq!(steps.len(), 1);
            }
            other => panic!("expected Loop step, got {:?}", other),
        }
    }

    #[test]
    fn parse_procedure_with_emit() {
        let source =
            "procedure notify:\n  trigger: manual\n  emit {type: \"alert\", level: \"high\"}\n";

        let doc = parse(source).expect("parse failed");
        assert_eq!(doc.procedures.len(), 1);
        match &doc.procedures[0].steps[0] {
            PxStep::Emit { event } => {
                assert_eq!(event["type"], "alert");
                assert_eq!(event["level"], "high");
            }
            other => panic!("expected Emit step, got {:?}", other),
        }
    }

    #[test]
    fn parse_procedure_with_try_catch() {
        let source = "procedure resilient:\n  trigger: manual\n  try:\n    risky_action {}\n  catch:\n    fallback {}\n  end\n";

        let doc = parse(source).expect("parse failed");
        assert_eq!(doc.procedures.len(), 1);
        match &doc.procedures[0].steps[0] {
            PxStep::Try { steps, catch } => {
                assert_eq!(steps.len(), 1);
                assert_eq!(catch.len(), 1);
            }
            other => panic!("expected Try step, got {:?}", other),
        }
    }

    #[test]
    fn parse_procedure_with_parallel() {
        let source = "procedure fan_out:\n  trigger: manual\n  parallel -> $results:\n    branch fetch_users:\n      get_users {}\n    end\n    branch fetch_posts:\n      get_posts {}\n    end\n  end\n";

        let doc = parse(source).expect("parse failed");
        assert_eq!(doc.procedures.len(), 1);
        match &doc.procedures[0].steps[0] {
            PxStep::Parallel { branches, output_var } => {
                assert_eq!(branches.len(), 2);
                assert_eq!(branches[0].name, "fetch_users");
                assert_eq!(branches[1].name, "fetch_posts");
                assert_eq!(branches[0].steps.len(), 1);
                assert_eq!(branches[1].steps.len(), 1);
                assert_eq!(output_var.as_deref(), Some("results"));
            }
            other => panic!("expected Parallel step, got {:?}", other),
        }
    }

    #[test]
    fn full_pipeline_loop_emit_try() {
        // Parse → Compile → Execute with all new step kinds
        use crate::px::compiler::compile;
        use crate::px::executor::{self, ActionHandler, ExecutionError};
        use serde_json::{json, Value};

        struct TestHandler;
        impl ActionHandler for TestHandler {
            fn call(&self, name: &str, _params: &Value) -> Result<Value, ExecutionError> {
                match name {
                    "get_items" => Ok(json!(["a", "b", "c"])),
                    "transform" => Ok(json!("done")),
                    _ => Err(ExecutionError::UnknownAction(name.to_string())),
                }
            }
        }

        let source = "procedure pipeline:\n  trigger: manual\n  get_items {} -> $items\n  loop over $items as item -> $results:\n    transform {val: $item}\n  end\n  emit {type: \"complete\", count: 3}\n";

        let doc = parse(source).expect("parse failed");
        let records = compile(&doc);
        assert_eq!(records.len(), 1);

        let result = executor::execute(&records[0].data, &TestHandler).unwrap();
        assert!(result.success);
        assert_eq!(
            result.variables.get("results"),
            Some(&json!(["done", "done", "done"]))
        );
        // Check emit was captured
        let emit = result.variables.get("emit").unwrap().as_array().unwrap();
        assert_eq!(emit.len(), 1);
        assert_eq!(emit[0]["type"], "complete");
    }
}

#[cfg(test)]
mod parse_value_tests {
    use super::*;
    use pest::Parser;

    #[test]
    fn parse_var_ref() {
        let r = PxParser::parse(Rule::var_ref, "$item");
        assert!(r.is_ok(), "var_ref failed: {:?}", r.err());
    }

    #[test]
    fn parse_value_with_var_ref() {
        let r = PxParser::parse(Rule::value, "$item");
        assert!(r.is_ok(), "value($item) failed: {:?}", r.err());
    }

    #[test]
    fn parse_map_val_with_var_ref() {
        let r = PxParser::parse(Rule::map_val, "{val: $item}");
        assert!(r.is_ok(), "map_val failed: {:?}", r.err());
    }
}

#[cfg(test)]
mod parse_step_tests {
    use super::*;

    #[test]
    fn parse_procedure_call_with_var_ref_in_map() {
        let source = "procedure test:\n  trigger: manual\n  do_thing {val: $foo}\n";
        let doc = parse(source).expect("parse failed");
        assert_eq!(doc.procedures.len(), 1);
        assert_eq!(doc.procedures[0].steps.len(), 1);
    }
}
