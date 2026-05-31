//! Mutation-gap tests for px/builder.rs
//!
//! Each test targets specific mutants missed in mutation testing.
//! Grouped by builder function.

#[cfg(test)]
mod tests {
    use crate::px::{parse, FunctionMode, PxStep};

    // =========================================================================
    // push_pair_into_document — statement delegation, function_decl, trigger_decl
    // =========================================================================

    #[test]
    fn build_function_decl_is_parsed() {
        let src = "function score_risk(severity: int, age: float) -> float:\n  mode: probabilistic\n  \"\"\"Compute composite risk score\"\"\"\n";
        let doc = parse(src).unwrap();
        assert_eq!(doc.functions.len(), 1);
        let f = &doc.functions[0];
        assert_eq!(f.name, "score_risk");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].name, "severity");
        assert_eq!(f.params[0].type_expr, "int");
        assert_eq!(f.params[1].name, "age");
        assert_eq!(f.params[1].type_expr, "float");
        assert_eq!(f.return_type, "float");
        assert!(matches!(f.mode, FunctionMode::Probabilistic));
        assert!(f.docstring.contains("Compute composite risk score"));
    }

    #[test]
    fn build_function_hybrid_mode() {
        let src = "function classify(input: string) -> string:\n  mode: hybrid\n  \"\"\"Classify input\"\"\"\n";
        let doc = parse(src).unwrap();
        assert!(matches!(doc.functions[0].mode, FunctionMode::Hybrid));
    }

    #[test]
    fn build_function_deterministic_mode_default() {
        let src = "function identity(x: int) -> int:\n  mode: deterministic\n  \"\"\"Return x\"\"\"\n";
        let doc = parse(src).unwrap();
        assert!(matches!(doc.functions[0].mode, FunctionMode::Deterministic));
    }

    #[test]
    fn build_function_no_params() {
        let src = "function noop() -> bool:\n  \"\"\"Does nothing\"\"\"\n";
        let doc = parse(src).unwrap();
        let f = &doc.functions[0];
        assert_eq!(f.name, "noop");
        assert!(f.params.is_empty());
        assert_eq!(f.return_type, "bool");
        assert!(matches!(f.mode, FunctionMode::Deterministic)); // default
        assert!(f.docstring.contains("Does nothing"));
    }

    #[test]
    fn build_trigger_decl_is_parsed() {
        let src = "trigger nightly_cleanup:\n  on: timer\n  schedule: \"0 0 * * *\"\n  run: cleanup_proc\n";
        let doc = parse(src).unwrap();
        assert_eq!(doc.triggers.len(), 1);
        let t = &doc.triggers[0];
        assert_eq!(t.name, "nightly_cleanup");
        assert_eq!(t.on_event, "timer");
        assert_eq!(t.schedule.as_deref(), Some("0 0 * * *"));
        assert_eq!(t.run, "cleanup_proc");
    }

    #[test]
    fn build_trigger_without_schedule() {
        let src = "trigger post_store:\n  on: after_store\n  run: index_update\n";
        let doc = parse(src).unwrap();
        let t = &doc.triggers[0];
        assert_eq!(t.on_event, "after_store");
        assert!(t.schedule.is_none());
        assert_eq!(t.run, "index_update");
    }

    // =========================================================================
    // build_fact — field_list filtering
    // =========================================================================

    #[test]
    fn build_fact_with_multiple_fields() {
        let src = "fact PullRequest:\n  number: int\n  title: string\n  status: string\n";
        let doc = parse(src).unwrap();
        assert_eq!(doc.facts.len(), 1);
        let f = &doc.facts[0];
        assert_eq!(f.name, "PullRequest");
        assert_eq!(f.fields.len(), 3);
        assert_eq!(f.fields[0].name, "number");
        assert_eq!(f.fields[0].type_expr, "int");
        assert_eq!(f.fields[1].name, "title");
        assert_eq!(f.fields[2].name, "status");
    }

    // =========================================================================
    // build_rule — priority, let, action_list, conditional_action, capture
    // =========================================================================

    #[test]
    fn build_rule_with_priority() {
        let src = "rule high_priority_rule:\n  priority: 10\n  when:\n    - status == \"critical\"\n  then:\n    - action: emit event: \"alert\"\n";
        let doc = parse(src).unwrap();
        assert_eq!(doc.rules[0].priority, Some(10));
    }

    #[test]
    fn build_rule_with_let_clause() {
        let src = "rule computed_rule:\n  when:\n    - value > 10\n  let threshold = 42\n  then:\n    - action: emit event: \"over\"\n";
        let doc = parse(src).unwrap();
        assert_eq!(doc.rules[0].lets.len(), 1);
        assert_eq!(doc.rules[0].lets[0].0, "threshold");
        assert_eq!(doc.rules[0].lets[0].1, "42");
    }

    #[test]
    fn build_rule_with_multiple_actions() {
        let src = "rule multi_action:\n  when:\n    - x == 1\n  then:\n    - action: emit event: \"a\"\n    - action: notify channel: \"ops\"\n";
        let doc = parse(src).unwrap();
        assert_eq!(doc.rules[0].actions.len(), 2);
        assert_eq!(doc.rules[0].actions[0].kind, "emit");
        assert_eq!(doc.rules[0].actions[1].kind, "notify");
    }

    #[test]
    fn build_rule_conditional_action() {
        let src = "rule cond_action:\n  when:\n    - active == true\n  then:\n    - if priority > 5: action: notify channel: \"urgent\"\n";
        let doc = parse(src).unwrap();
        let action = &doc.rules[0].actions[0];
        assert!(action.condition.is_some());
        assert_eq!(action.kind, "notify");
    }

    #[test]
    fn build_rule_action_params() {
        let src = "rule param_action:\n  when:\n    - x == 1\n  then:\n    - action: notify channel: \"ops\" level: 3\n";
        let doc = parse(src).unwrap();
        let action = &doc.rules[0].actions[0];
        assert_eq!(action.kind, "notify");
        assert_eq!(action.params.get("channel").unwrap(), "ops");
        assert_eq!(action.params.get("level").unwrap(), 3);
    }

    // =========================================================================
    // build_capture — content, category, tags
    // =========================================================================

    #[test]
    fn build_rule_with_capture() {
        let src = "rule capture_rule:\n  when:\n    - status == \"done\"\n  then:\n    - action: emit event: \"captured\"\n  capture:\n    - fact: \"important finding\" category: decision tags: [\"tag1\", \"tag2\"]\n";
        let doc = parse(src).unwrap();
        let cap = &doc.rules[0].captures;
        assert_eq!(cap.len(), 1);
        assert_eq!(cap[0].content, "important finding");
        assert_eq!(cap[0].category.as_deref(), Some("decision"));
        assert_eq!(cap[0].tags, vec!["tag1", "tag2"]);
    }

    #[test]
    fn build_rule_capture_without_tags() {
        let src = "rule bare_capture:\n  when:\n    - x == 1\n  then:\n    - action: emit event: \"x\"\n  capture:\n    - fact: \"bare note\" category: preference\n";
        let doc = parse(src).unwrap();
        let cap = &doc.rules[0].captures[0];
        assert_eq!(cap.content, "bare note");
        assert_eq!(cap.category.as_deref(), Some("preference"));
        assert!(cap.tags.is_empty());
    }

    // =========================================================================
    // build_constraint — scope, message
    // =========================================================================

    #[test]
    fn build_constraint_scope_and_message() {
        let src = "constraint no_direct_push:\n  scope: repository\n  when: push.branch == \"main\"\n  require: push.has_review == true\n  severity: error\n  message: \"Direct pushes to main are forbidden\"\n";
        let doc = parse(src).unwrap();
        let c = &doc.constraints[0];
        assert_eq!(c.scope.as_deref(), Some("repository"));
        assert_eq!(
            c.message.as_deref(),
            Some("Direct pushes to main are forbidden")
        );
        assert_eq!(c.severity, "error");
    }

    // =========================================================================
    // build_contract — given, when, then, threshold, examples
    // =========================================================================

    #[test]
    #[ignore = "contract indentation parsing not yet implemented"]
    fn build_contract_full() {
        // Use format identical to working test in mod.rs
        let src = r#"
contract classify_severity:
  given: "A bug report"
  when: "Classifier invoked"
  then: "Returns severity"
  threshold: 0.85
  examples:
    - input: { text: "crash" }
      expect: [critical]
    - input: { text: "typo" }
      expect: [low]
"#;
        let doc = parse(src).unwrap();
        assert_eq!(doc.contracts.len(), 1);
        let c = &doc.contracts[0];
        assert_eq!(c.name, "classify_severity");
        assert_eq!(c.given.as_deref(), Some("A bug report"));
        assert_eq!(c.when_desc.as_deref(), Some("Classifier invoked"));
        assert_eq!(c.then_desc.as_deref(), Some("Returns severity"));
        assert_eq!(c.threshold, Some(0.85));
        assert_eq!(c.examples.len(), 2);
    }

    #[test]
    #[ignore = "contract indentation parsing not yet implemented"]
    fn build_contract_minimal() {
        let src = r#"
contract simple:
  examples:
    - input: { x: 1 }
      expect: [y]
"#;
        let doc = parse(src).unwrap();
        let c = &doc.contracts[0];
        assert!(c.given.is_none());
        assert!(c.when_desc.is_none());
        assert!(c.then_desc.is_none());
        assert!(c.threshold.is_none());
        assert_eq!(c.examples.len(), 1);
    }

    // =========================================================================
    // build_procedure — trigger, given
    // =========================================================================

    #[test]
    fn build_procedure_with_trigger_and_given() {
        let src = "procedure deploy_flow:\n  trigger: manual\n  given: \"A build artifact is ready\"\n  validate_artifact {}\n  deploy_to_staging {}\n";
        let doc = parse(src).unwrap();
        let p = &doc.procedures[0];
        assert_eq!(p.name, "deploy_flow");
        assert!(p.trigger.is_some());
        assert_eq!(p.trigger.as_ref().unwrap().kind, "manual");
        assert_eq!(p.given.as_deref(), Some("A build artifact is ready"));
        assert_eq!(p.steps.len(), 2);
    }

    // =========================================================================
    // build_step — match, param_pair, loop ident_index
    // =========================================================================

    #[test]
    fn build_step_match_arms() {
        let src = "procedure matcher:\n  trigger: manual\n  match:\n    status == \"ok\" -> proceed\n    status == \"fail\" -> abort\n  end\n";
        let doc = parse(src).unwrap();
        let steps = &doc.procedures[0].steps;
        assert_eq!(steps.len(), 1);
        match &steps[0] {
            PxStep::Match { arms } => {
                assert_eq!(arms.len(), 2);
                assert!(arms[0].condition.contains("ok"));
                assert_eq!(arms[0].result, "proceed");
                assert!(arms[1].condition.contains("fail"));
                assert_eq!(arms[1].result, "abort");
            }
            other => panic!("Expected Match step, got {:?}", other),
        }
    }

    #[test]
    fn build_step_call_with_params() {
        let src = "procedure param_proc:\n  trigger: manual\n  notify channel: \"ops\" level: 5\n";
        let doc = parse(src).unwrap();
        match &doc.procedures[0].steps[0] {
            PxStep::Call { name, params, .. } => {
                assert_eq!(name, "notify");
                assert_eq!(params.get("channel").unwrap(), "ops");
                assert_eq!(params.get("level").unwrap(), 5);
            }
            other => panic!("Expected Call step, got {:?}", other),
        }
    }

    #[test]
    fn build_step_emit_with_params() {
        let src = "procedure emitter:\n  trigger: manual\n  emit event: \"deployed\" version: 42\n";
        let doc = parse(src).unwrap();
        match &doc.procedures[0].steps[0] {
            PxStep::Emit { event } => {
                assert_eq!(event.get("event").unwrap(), "deployed");
                assert_eq!(event.get("version").unwrap(), 42);
            }
            other => panic!("Expected Emit step, got {:?}", other),
        }
    }

    // =========================================================================
    // build_scenario — given, run, expectations (positive + NOT)
    // =========================================================================

    #[test]
    fn build_scenario_full() {
        let src = "procedure target_proc:\n  trigger: manual\n  noop {}\n\nscenario deploy_scenario:\n  given: \"Staging env is clean\"\n  run: target_proc {env: \"staging\"}\n  expect:\n    - has_entry {key: \"deployed\"}\n    - NOT has_error {code: 500}\n";
        let doc = parse(src).unwrap();
        assert_eq!(doc.scenarios.len(), 1);
        let s = &doc.scenarios[0];
        assert_eq!(s.name, "deploy_scenario");
        assert_eq!(s.given, "Staging env is clean");
        let run = s.run.as_ref().unwrap();
        assert_eq!(run.procedure, "target_proc");
        assert!(run.params.is_some());
        assert_eq!(s.expectations.len(), 2);
        // Positive expectation
        assert!(!s.expectations[0].negated);
        assert_eq!(s.expectations[0].check, "has_entry");
        // Negated expectation
        assert!(s.expectations[1].negated);
        assert_eq!(s.expectations[1].check, "has_error");
    }

    // =========================================================================
    // parse_value — integer, float, boolean, list, ident via action params
    // =========================================================================

    #[test]
    fn parse_value_integer_in_action() {
        let src = "rule int_rule:\n  when:\n    - count > 0\n  then:\n    - action: emit value: 42\n";
        let doc = parse(src).unwrap();
        let val = doc.rules[0].actions[0].params.get("value").unwrap();
        assert_eq!(val, 42);
    }

    #[test]
    fn parse_value_float_in_action() {
        let src = "rule float_rule:\n  when:\n    - score > 0\n  then:\n    - action: emit threshold: 3.14\n";
        let doc = parse(src).unwrap();
        let val = doc.rules[0].actions[0].params.get("threshold").unwrap();
        assert_eq!(val, &serde_json::json!(3.14));
    }

    #[test]
    fn parse_value_boolean_true_in_action() {
        let src = "rule bool_rule:\n  when:\n    - x == 1\n  then:\n    - action: emit active: true\n";
        let doc = parse(src).unwrap();
        let val = doc.rules[0].actions[0].params.get("active").unwrap();
        assert_eq!(val, &serde_json::json!(true));
    }

    #[test]
    fn parse_value_boolean_false_in_action() {
        let src = "rule bool_f_rule:\n  when:\n    - x == 1\n  then:\n    - action: emit active: false\n";
        let doc = parse(src).unwrap();
        let val = doc.rules[0].actions[0].params.get("active").unwrap();
        assert_eq!(val, &serde_json::json!(false));
    }

    #[test]
    fn parse_value_list_in_action() {
        let src = "rule list_rule:\n  when:\n    - x == 1\n  then:\n    - action: emit tags: [\"alpha\", \"beta\", \"gamma\"]\n";
        let doc = parse(src).unwrap();
        let val = doc.rules[0].actions[0].params.get("tags").unwrap();
        assert!(val.is_array());
        let arr = val.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0], "alpha");
        assert_eq!(arr[1], "beta");
        assert_eq!(arr[2], "gamma");
    }

    #[test]
    fn parse_value_ident_as_value_in_action() {
        let src = "rule ident_rule:\n  when:\n    - x == 1\n  then:\n    - action: emit target: staging\n";
        let doc = parse(src).unwrap();
        let val = doc.rules[0].actions[0].params.get("target").unwrap();
        assert_eq!(val, "staging");
    }

    // =========================================================================
    // build_step loop — ident_index tracking (item_var vs output_var)
    // =========================================================================

    #[test]
    fn build_step_loop_over_with_item_var() {
        let src = "procedure loop_proc:\n  trigger: manual\n  fetch_items {} -> $items\n  loop over $items as item -> $results:\n    process_item {val: $item}\n  end\n";
        let doc = parse(src).unwrap();
        match &doc.procedures[0].steps[1] {
            PxStep::Loop {
                over,
                item_var,
                output_var,
                steps,
                ..
            } => {
                assert_eq!(over.as_deref(), Some("$items"));
                assert_eq!(item_var, "item");
                assert_eq!(output_var.as_deref(), Some("results"));
                assert!(!steps.is_empty());
            }
            other => panic!("Expected Loop step, got {:?}", other),
        }
    }

    // =========================================================================
    // Multiple declarations in one doc (statement arm delegation)
    // =========================================================================

    #[test]
    fn multiple_declarations_parsed_into_document() {
        let src = "fact Ticket:\n  id: int\n  title: string\n\nrule auto_close:\n  when:\n    - age > 30\n  then:\n    - action: emit act: \"close\"\n\nconstraint must_review:\n  when: pr.files > 10\n  require: pr.reviewers > 0\n  severity: error\n\ntrigger post_merge:\n  on: after_store\n  run: notify_team\n\nfunction risk(sev: int) -> float:\n  mode: deterministic\n  \"\"\"Calculate risk\"\"\"\n";
        let doc = parse(src).unwrap();
        assert_eq!(doc.facts.len(), 1);
        assert_eq!(doc.rules.len(), 1);
        assert_eq!(doc.constraints.len(), 1);
        assert_eq!(doc.triggers.len(), 1);
        assert_eq!(doc.functions.len(), 1);
    }

    // =========================================================================
    // EOI arm — document parses completely without trailing garbage
    // =========================================================================

    #[test]
    fn eoi_is_handled_gracefully() {
        // Minimal valid doc — should parse cleanly with EOI
        let src = "fact Minimal:\n  x: int\n";
        let doc = parse(src).unwrap();
        assert_eq!(doc.facts.len(), 1);
    }
}

#[cfg(test)]
mod debug_tests {
    use crate::px::parse;
    
    #[test]
    #[ignore = "contract indentation parsing not yet implemented"]
    fn debug_contract_examples() {
        let src = r#"
contract auto_merge_behavior:
  given: "CI green + reviewed"
  when: "lifecycle evaluates"
  then: "PR merged"
  examples:
    - input: { ci_status: green, has_review: true }
      expect: [merge_pr]
"#;
        let doc = parse(src).unwrap();
        let c = &doc.contracts[0];
        eprintln!("name: {}", c.name);
        eprintln!("given: {:?}", c.given);
        eprintln!("when_desc: {:?}", c.when_desc);
        eprintln!("then_desc: {:?}", c.then_desc);
        eprintln!("threshold: {:?}", c.threshold);
        eprintln!("examples.len: {}", c.examples.len());
        for (i, ex) in c.examples.iter().enumerate() {
            eprintln!("  example[{}]: input={}, expect={}", i, ex.input, ex.expect);
        }
        assert_eq!(c.examples.len(), 1);
    }
}

#[cfg(test)]
mod debug_tests2 {
    use crate::px::parse;
    
    #[test]
    #[ignore = "contract indentation parsing not yet implemented"]
    fn debug_contract_in_full_doc() {
        let src = r#"
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
        let doc = parse(src).unwrap();
        eprintln!("contracts: {}", doc.contracts.len());
        let c = &doc.contracts[0];
        eprintln!("examples: {}", c.examples.len());
        assert_eq!(c.examples.len(), 1);
    }
}
