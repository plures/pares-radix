//! Lint pass for .px documents — detects potential issues before execution.
//!
//! Produces warnings/errors for patterns that may cause runtime failures:
//! - Non-exhaustive match steps (no wildcard `_` arm)
//! - Empty procedure bodies
//! - Unreachable code after unconditional match arms

use super::{PxDocument, PxMatchArm, PxProcedure, PxStep};

/// Severity of a lint diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Warning,
    Error,
}

/// A lint diagnostic produced by the lint pass.
#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    /// Which lint rule triggered this.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Severity level.
    pub severity: LintSeverity,
    /// Name of the procedure (if applicable).
    pub procedure: Option<String>,
    /// Step index within the procedure (0-based, if applicable).
    pub step_index: Option<usize>,
}

impl std::fmt::Display for LintDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = match self.severity {
            LintSeverity::Warning => "warning",
            LintSeverity::Error => "error",
        };
        let location = match (&self.procedure, self.step_index) {
            (Some(proc), Some(idx)) => format!(" in `{}` step {}", proc, idx + 1),
            (Some(proc), None) => format!(" in `{}`", proc),
            _ => String::new(),
        };
        write!(f, "[{}] {}{}: {}", self.code, sev, location, self.message)
    }
}

/// Run all lint passes on a parsed document.
pub fn lint(doc: &PxDocument) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();

    for procedure in &doc.procedures {
        lint_procedure(procedure, &mut diagnostics);
    }

    diagnostics
}

/// Lint a single procedure.
fn lint_procedure(proc: &PxProcedure, diags: &mut Vec<LintDiagnostic>) {
    // L001: Empty procedure body
    if proc.steps.is_empty() {
        diags.push(LintDiagnostic {
            code: "PX-L001",
            message: "procedure has no steps".to_string(),
            severity: LintSeverity::Warning,
            procedure: Some(proc.name.clone()),
            step_index: None,
        });
        return;
    }

    for (idx, step) in proc.steps.iter().enumerate() {
        lint_step(step, &proc.name, idx, diags);
    }

    // L005: Unused output variables (procedure-level analysis)
    lint_unused_output_vars(proc, diags);

    // L008: Shadowed output variables (same name bound by multiple steps)
    lint_shadowed_output_vars(proc, diags);
}

/// Lint a single step (recursing into nested structures).
fn lint_step(step: &PxStep, proc_name: &str, idx: usize, diags: &mut Vec<LintDiagnostic>) {
    match step {
        PxStep::Match { arms } => {
            lint_match_exhaustiveness(arms, proc_name, idx, diags);
            lint_match_unreachable(arms, proc_name, idx, diags);
            lint_match_duplicate_conditions(arms, proc_name, idx, diags);
        }
        PxStep::Loop {
            over,
            item_var,
            key_var,
            steps,
            ..
        } => {
            lint_unused_loop_item_var(over, item_var, key_var, steps, proc_name, idx, diags);
            for (sub_idx, sub_step) in steps.iter().enumerate() {
                lint_step(sub_step, proc_name, sub_idx, diags);
            }
        }
        PxStep::When { steps, .. } => {
            for (sub_idx, sub_step) in steps.iter().enumerate() {
                lint_step(sub_step, proc_name, sub_idx, diags);
            }
        }
        PxStep::Try { steps, catch, .. } => {
            lint_empty_catch(catch, proc_name, idx, diags);
            for (sub_idx, sub_step) in steps.iter().enumerate() {
                lint_step(sub_step, proc_name, sub_idx, diags);
            }
            for (sub_idx, sub_step) in catch.iter().enumerate() {
                lint_step(sub_step, proc_name, sub_idx, diags);
            }
        }
        PxStep::Parallel { branches, .. } => {
            for branch in branches {
                for (sub_idx, sub_step) in branch.steps.iter().enumerate() {
                    lint_step(sub_step, proc_name, sub_idx, diags);
                }
            }
        }
        _ => {}
    }
}

/// PX-L002: Non-exhaustive match — no wildcard `_` arm present.
fn lint_match_exhaustiveness(
    arms: &[PxMatchArm],
    proc_name: &str,
    idx: usize,
    diags: &mut Vec<LintDiagnostic>,
) {
    let has_wildcard = arms.iter().any(|arm| {
        let cond = arm.condition.trim();
        cond == "_" || cond == "_ =>" || cond.starts_with("_ ")
    });

    if !has_wildcard {
        diags.push(LintDiagnostic {
            code: "PX-L002",
            message: format!(
                "match step has {} arm(s) but no wildcard `_` — may fail at runtime if no arm matches",
                arms.len()
            ),
            severity: LintSeverity::Warning,
            procedure: Some(proc_name.to_string()),
            step_index: Some(idx),
        });
    }
}

/// PX-L004: Duplicate arm conditions in a match.
fn lint_match_duplicate_conditions(
    arms: &[PxMatchArm],
    proc_name: &str,
    idx: usize,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (arm_idx, arm) in arms.iter().enumerate() {
        let cond = arm.condition.trim();
        if cond == "_" {
            continue; // wildcard is a special case, not a duplicate
        }
        if let Some(&first_idx) = seen.get(cond) {
            diags.push(LintDiagnostic {
                code: "PX-L004",
                message: format!(
                    "arm {} has the same condition as arm {} (`{}`) — only the first will ever match",
                    arm_idx + 1,
                    first_idx + 1,
                    cond
                ),
                severity: LintSeverity::Warning,
                procedure: Some(proc_name.to_string()),
                step_index: Some(idx),
            });
        } else {
            seen.insert(cond, arm_idx);
        }
    }
}

/// PX-L005: Unused output variables — bound but never referenced in subsequent steps.
fn lint_unused_output_vars(proc: &PxProcedure, diags: &mut Vec<LintDiagnostic>) {
    // Collect all output_var bindings with their step index
    let mut bindings: Vec<(usize, &str)> = Vec::new();
    for (idx, step) in proc.steps.iter().enumerate() {
        if let Some(var) = step_output_var(step) {
            bindings.push((idx, var));
        }
    }

    if bindings.is_empty() {
        return;
    }

    // Collect all variable references across the procedure
    let mut references: std::collections::HashSet<String> = std::collections::HashSet::new();
    for step in &proc.steps {
        collect_var_references(step, &mut references);
    }

    // Check each binding against references
    for (idx, var_name) in bindings {
        if !references.contains(&format!("${}", var_name)) {
            diags.push(LintDiagnostic {
                code: "PX-L005",
                message: format!(
                    "output variable `${}` is bound but never referenced in subsequent steps",
                    var_name
                ),
                severity: LintSeverity::Warning,
                procedure: Some(proc.name.clone()),
                step_index: Some(idx),
            });
        }
    }
}

/// Extract the output_var from a step, if any.
fn step_output_var(step: &PxStep) -> Option<&str> {
    match step {
        PxStep::Call { output_var, .. } => output_var.as_deref(),
        PxStep::Loop { output_var, .. } => output_var.as_deref(),
        PxStep::Parallel { output_var, .. } => output_var.as_deref(),
        _ => None,
    }
}

/// Recursively collect all `$variable` references from a step.
fn collect_var_references(step: &PxStep, refs: &mut std::collections::HashSet<String>) {
    match step {
        PxStep::Call { params, .. } => {
            collect_refs_from_value(params, refs);
        }
        PxStep::Match { arms } => {
            for arm in arms {
                collect_refs_from_str(&arm.condition, refs);
                collect_refs_from_str(&arm.result, refs);
            }
        }
        PxStep::When { condition, steps } => {
            collect_refs_from_str(condition, refs);
            for s in steps {
                collect_var_references(s, refs);
            }
        }
        PxStep::Loop { over, steps, .. } => {
            if let Some(over_expr) = over {
                collect_refs_from_str(over_expr, refs);
            }
            for s in steps {
                collect_var_references(s, refs);
            }
        }
        PxStep::Emit { event } => {
            collect_refs_from_value(event, refs);
        }
        PxStep::Try { steps, catch, .. } => {
            for s in steps {
                collect_var_references(s, refs);
            }
            for s in catch {
                collect_var_references(s, refs);
            }
        }
        PxStep::Parallel { branches, .. } => {
            for branch in branches {
                for s in &branch.steps {
                    collect_var_references(s, refs);
                }
            }
        }
    }
}

/// Extract `$identifier` patterns from a string.
fn collect_refs_from_str(s: &str, refs: &mut std::collections::HashSet<String>) {
    // Match $identifier patterns (alphanumeric + underscore, starting with $)
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' {
            let mut var = String::from("$");
            while let Some(&next) = chars.peek() {
                if next.is_alphanumeric() || next == '_' {
                    var.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if var.len() > 1 {
                refs.insert(var);
            }
        }
    }
}

/// Extract `$identifier` patterns from a JSON value (recursing into objects/arrays/strings).
fn collect_refs_from_value(val: &serde_json::Value, refs: &mut std::collections::HashSet<String>) {
    match val {
        serde_json::Value::String(s) => collect_refs_from_str(s, refs),
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_refs_from_value(v, refs);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_refs_from_value(v, refs);
            }
        }
        _ => {}
    }
}

/// PX-L006: Unused loop item variable — loop iterates but never references the item.
fn lint_unused_loop_item_var(
    over: &Option<String>,
    item_var: &str,
    key_var: &Option<String>,
    steps: &[PxStep],
    proc_name: &str,
    idx: usize,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Only applies to `over` loops (not `times` loops which may just repeat N times)
    if over.is_none() {
        return;
    }

    let mut refs: std::collections::HashSet<String> = std::collections::HashSet::new();
    for step in steps {
        collect_var_references(step, &mut refs);
    }

    let item_ref = format!("${}", item_var);
    if !refs.contains(&item_ref) {
        diags.push(LintDiagnostic {
            code: "PX-L006",
            message: format!(
                "loop item variable `${}` is never referenced in loop body — consider using `times` instead of `over`",
                item_var
            ),
            severity: LintSeverity::Warning,
            procedure: Some(proc_name.to_string()),
            step_index: Some(idx),
        });
    }

    // Also check key_var if declared
    if let Some(kv) = key_var {
        let key_ref = format!("${}", kv);
        if !refs.contains(&key_ref) {
            diags.push(LintDiagnostic {
                code: "PX-L006",
                message: format!(
                    "loop key variable `${}` is declared but never referenced in loop body",
                    kv
                ),
                severity: LintSeverity::Warning,
                procedure: Some(proc_name.to_string()),
                step_index: Some(idx),
            });
        }
    }
}

/// PX-L007: Empty catch block — errors are silently swallowed.
fn lint_empty_catch(
    catch: &[PxStep],
    proc_name: &str,
    idx: usize,
    diags: &mut Vec<LintDiagnostic>,
) {
    if catch.is_empty() {
        diags.push(LintDiagnostic {
            code: "PX-L007",
            message: "try step has an empty catch block — errors will be silently swallowed"
                .to_string(),
            severity: LintSeverity::Warning,
            procedure: Some(proc_name.to_string()),
            step_index: Some(idx),
        });
    }
}

/// PX-L003: Unreachable arms after a wildcard `_`.
fn lint_match_unreachable(
    arms: &[PxMatchArm],
    proc_name: &str,
    idx: usize,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut wildcard_seen = false;
    for (arm_idx, arm) in arms.iter().enumerate() {
        let cond = arm.condition.trim();
        if wildcard_seen {
            diags.push(LintDiagnostic {
                code: "PX-L003",
                message: format!(
                    "arm {} is unreachable — wildcard `_` already covers all cases (arm {})",
                    arm_idx + 1,
                    arm_idx
                ),
                severity: LintSeverity::Warning,
                procedure: Some(proc_name.to_string()),
                step_index: Some(idx),
            });
        }
        if cond == "_" || cond.starts_with("_ ") {
            wildcard_seen = true;
        }
    }
}

/// PX-L008: Shadowed output variables — multiple steps bind to the same output_var name.
///
/// The later binding overwrites the earlier one, making the first call's output
/// inaccessible. This is usually a copy-paste bug.
fn lint_shadowed_output_vars(proc: &PxProcedure, diags: &mut Vec<LintDiagnostic>) {
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

    for (idx, step) in proc.steps.iter().enumerate() {
        if let Some(var) = step_output_var(step) {
            if let Some(&first_idx) = seen.get(var) {
                diags.push(LintDiagnostic {
                    code: "PX-L008",
                    message: format!(
                        "output variable `${}` is already bound by step {} — this binding shadows it",
                        var,
                        first_idx + 1
                    ),
                    severity: LintSeverity::Warning,
                    procedure: Some(proc.name.clone()),
                    step_index: Some(idx),
                });
            } else {
                seen.insert(var, idx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::px::{PxDocument, PxMatchArm, PxProcedure, PxProcedureTrigger, PxStep};

    fn empty_doc() -> PxDocument {
        PxDocument {
            imports: vec![],
            facts: vec![],
            rules: vec![],
            constraints: vec![],
            contracts: vec![],
            functions: vec![],
            triggers: vec![],
            procedures: vec![],
        }
    }

    fn make_proc(name: &str, steps: Vec<PxStep>) -> PxProcedure {
        PxProcedure {
            name: name.to_string(),
            trigger: Some(PxProcedureTrigger {
                kind: "manual".to_string(),
                params: None,
            }),
            given: None,
            steps,
        }
    }

    #[test]
    fn l001_empty_procedure() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc("empty", vec![]));

        let diags = lint(&doc);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "PX-L001");
        assert_eq!(diags[0].severity, LintSeverity::Warning);
        assert_eq!(diags[0].procedure.as_deref(), Some("empty"));
    }

    #[test]
    fn l002_non_exhaustive_match() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "handler",
            vec![PxStep::Match {
                arms: vec![
                    PxMatchArm {
                        condition: "status == \"active\"".to_string(),
                        result: "active".to_string(),
                    },
                    PxMatchArm {
                        condition: "status == \"inactive\"".to_string(),
                        result: "inactive".to_string(),
                    },
                ],
            }],
        ));

        let diags = lint(&doc);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "PX-L002");
        assert!(diags[0].message.contains("no wildcard"));
    }

    #[test]
    fn l002_exhaustive_match_no_warning() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "handler",
            vec![PxStep::Match {
                arms: vec![
                    PxMatchArm {
                        condition: "status == \"active\"".to_string(),
                        result: "active".to_string(),
                    },
                    PxMatchArm {
                        condition: "_".to_string(),
                        result: "unknown".to_string(),
                    },
                ],
            }],
        ));

        let diags = lint(&doc);
        assert!(diags.is_empty());
    }

    #[test]
    fn l003_unreachable_after_wildcard() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "handler",
            vec![PxStep::Match {
                arms: vec![
                    PxMatchArm {
                        condition: "status == \"active\"".to_string(),
                        result: "active".to_string(),
                    },
                    PxMatchArm {
                        condition: "_".to_string(),
                        result: "default".to_string(),
                    },
                    PxMatchArm {
                        condition: "status == \"pending\"".to_string(),
                        result: "pending".to_string(),
                    },
                ],
            }],
        ));

        let diags = lint(&doc);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "PX-L003");
        assert!(diags[0].message.contains("unreachable"));
    }

    #[test]
    fn lint_nested_match_in_loop() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "processor",
            vec![PxStep::Loop {
                over: Some("$items".to_string()),
                times: None,
                item_var: "item".to_string(),
                key_var: None,
                steps: vec![PxStep::Match {
                    arms: vec![PxMatchArm {
                        condition: "item.type == \"a\"".to_string(),
                        result: "handled".to_string(),
                    }],
                }],
                output_var: None,
            }],
        ));

        let diags = lint(&doc);
        // L002 for non-exhaustive match + L006 for unused $item (condition uses bare `item.type` not `$item`)
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().any(|d| d.code == "PX-L002"));
        assert!(diags.iter().any(|d| d.code == "PX-L006"));
    }

    #[test]
    fn lint_no_issues_for_simple_procedure() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "simple",
            vec![
                PxStep::Call {
                    name: "greet".to_string(),
                    params: serde_json::json!({}),
                    output_var: None,
                },
                PxStep::Emit {
                    event: serde_json::json!({"type": "done"}),
                },
            ],
        ));

        let diags = lint(&doc);
        assert!(diags.is_empty());
    }

    #[test]
    fn display_format() {
        let diag = LintDiagnostic {
            code: "PX-L002",
            message: "match step has 2 arm(s) but no wildcard `_`".to_string(),
            severity: LintSeverity::Warning,
            procedure: Some("handler".to_string()),
            step_index: Some(0),
        };
        let s = format!("{}", diag);
        assert!(s.contains("[PX-L002]"));
        assert!(s.contains("warning"));
        assert!(s.contains("handler"));
        assert!(s.contains("step 1"));
    }

    #[test]
    fn l004_duplicate_arm_conditions() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "handler",
            vec![PxStep::Match {
                arms: vec![
                    PxMatchArm {
                        condition: "status == \"active\"".to_string(),
                        result: "first".to_string(),
                    },
                    PxMatchArm {
                        condition: "status == \"pending\"".to_string(),
                        result: "second".to_string(),
                    },
                    PxMatchArm {
                        condition: "status == \"active\"".to_string(),
                        result: "duplicate".to_string(),
                    },
                    PxMatchArm {
                        condition: "_".to_string(),
                        result: "default".to_string(),
                    },
                ],
            }],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L004").collect();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("same condition as arm 1"));
    }

    #[test]
    fn l004_no_false_positive_for_unique_arms() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "handler",
            vec![PxStep::Match {
                arms: vec![
                    PxMatchArm {
                        condition: "status == \"a\"".to_string(),
                        result: "a".to_string(),
                    },
                    PxMatchArm {
                        condition: "status == \"b\"".to_string(),
                        result: "b".to_string(),
                    },
                    PxMatchArm {
                        condition: "_".to_string(),
                        result: "default".to_string(),
                    },
                ],
            }],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L004").collect();
        assert!(diags.is_empty());
    }

    #[test]
    fn l005_unused_output_var() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "pipeline",
            vec![
                PxStep::Call {
                    name: "fetch_data".to_string(),
                    params: serde_json::json!({}),
                    output_var: Some("data".to_string()),
                },
                PxStep::Emit {
                    event: serde_json::json!({"type": "done"}),
                },
            ],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L005").collect();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("$data"));
        assert!(diags[0].message.contains("never referenced"));
    }

    #[test]
    fn l005_no_warning_when_var_is_used() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "pipeline",
            vec![
                PxStep::Call {
                    name: "fetch_data".to_string(),
                    params: serde_json::json!({}),
                    output_var: Some("data".to_string()),
                },
                PxStep::Call {
                    name: "process".to_string(),
                    params: serde_json::json!({"input": "$data"}),
                    output_var: None,
                },
            ],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L005").collect();
        assert!(diags.is_empty());
    }

    #[test]
    fn l005_var_used_in_loop_over() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "pipeline",
            vec![
                PxStep::Call {
                    name: "get_items".to_string(),
                    params: serde_json::json!({}),
                    output_var: Some("items".to_string()),
                },
                PxStep::Loop {
                    over: Some("$items".to_string()),
                    times: None,
                    item_var: "item".to_string(),
                    key_var: None,
                    steps: vec![PxStep::Emit {
                        event: serde_json::json!({"item": "$item"}),
                    }],
                    output_var: None,
                },
            ],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L005").collect();
        assert!(diags.is_empty());
    }

    #[test]
    fn l005_var_used_in_when_condition() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "pipeline",
            vec![
                PxStep::Call {
                    name: "check".to_string(),
                    params: serde_json::json!({}),
                    output_var: Some("result".to_string()),
                },
                PxStep::When {
                    condition: "$result == true".to_string(),
                    steps: vec![PxStep::Emit {
                        event: serde_json::json!({"status": "ok"}),
                    }],
                },
            ],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L005").collect();
        assert!(diags.is_empty());
    }

    #[test]
    fn collect_refs_from_str_works() {
        let mut refs = std::collections::HashSet::new();
        collect_refs_from_str("hello $world and $foo_bar", &mut refs);
        assert!(refs.contains("$world"));
        assert!(refs.contains("$foo_bar"));
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn collect_refs_from_str_no_bare_dollar() {
        let mut refs = std::collections::HashSet::new();
        collect_refs_from_str("cost is $5 or $ nothing", &mut refs);
        // $5 starts with digit after $ but 5 is alphanumeric so it matches
        assert!(refs.contains("$5"));
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn l006_unused_loop_item_var() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "counter",
            vec![PxStep::Loop {
                over: Some("$items".to_string()),
                times: None,
                item_var: "item".to_string(),
                key_var: None,
                steps: vec![PxStep::Call {
                    name: "increment".to_string(),
                    params: serde_json::json!({"value": 1}),
                    output_var: None,
                }],
                output_var: None,
            }],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L006").collect();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("$item"));
        assert!(diags[0].message.contains("never referenced"));
    }

    #[test]
    fn l006_no_warning_when_item_used() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "processor",
            vec![PxStep::Loop {
                over: Some("$items".to_string()),
                times: None,
                item_var: "item".to_string(),
                key_var: None,
                steps: vec![PxStep::Call {
                    name: "process".to_string(),
                    params: serde_json::json!({"data": "$item"}),
                    output_var: None,
                }],
                output_var: None,
            }],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L006").collect();
        assert!(diags.is_empty());
    }

    #[test]
    fn l006_unused_key_var() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "mapper",
            vec![PxStep::Loop {
                over: Some("$map".to_string()),
                times: None,
                item_var: "val".to_string(),
                key_var: Some("key".to_string()),
                steps: vec![PxStep::Call {
                    name: "process".to_string(),
                    params: serde_json::json!({"data": "$val"}),
                    output_var: None,
                }],
                output_var: None,
            }],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L006").collect();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("$key"));
    }

    #[test]
    fn l006_no_warning_for_times_loop() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "repeater",
            vec![PxStep::Loop {
                over: None,
                times: Some(5),
                item_var: "i".to_string(),
                key_var: None,
                steps: vec![PxStep::Call {
                    name: "ping".to_string(),
                    params: serde_json::json!({}),
                    output_var: None,
                }],
                output_var: None,
            }],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L006").collect();
        assert!(diags.is_empty());
    }

    #[test]
    fn l007_empty_catch_block() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "risky",
            vec![PxStep::Try {
                steps: vec![PxStep::Call {
                    name: "risky_op".to_string(),
                    params: serde_json::json!({}),
                    output_var: None,
                }],
                catch: vec![],
                retry: None,
                retry_delay_ms: None,
                retry_backoff: None,
                retry_max_delay_ms: None,
                retry_jitter: None,
            }],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L007").collect();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("silently swallowed"));
    }

    #[test]
    fn l007_no_warning_with_catch_steps() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "safe",
            vec![PxStep::Try {
                steps: vec![PxStep::Call {
                    name: "risky_op".to_string(),
                    params: serde_json::json!({}),
                    output_var: None,
                }],
                catch: vec![PxStep::Emit {
                    event: serde_json::json!({"error": "handled"}),
                }],
                retry: None,
                retry_delay_ms: None,
                retry_backoff: None,
                retry_max_delay_ms: None,
                retry_jitter: None,
            }],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L007").collect();
        assert!(diags.is_empty());
    }

    #[test]
    fn l008_shadowed_output_var() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "pipeline",
            vec![
                PxStep::Call {
                    name: "fetch_data".to_string(),
                    params: serde_json::json!({}),
                    output_var: Some("result".to_string()),
                },
                PxStep::Call {
                    name: "transform_data".to_string(),
                    params: serde_json::json!({"input": "$result"}),
                    output_var: Some("result".to_string()),
                },
            ],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L008").collect();
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("$result"));
        assert!(diags[0].message.contains("step 1"));
        assert_eq!(diags[0].step_index, Some(1));
    }

    #[test]
    fn l008_no_warning_for_unique_output_vars() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "pipeline",
            vec![
                PxStep::Call {
                    name: "fetch_data".to_string(),
                    params: serde_json::json!({}),
                    output_var: Some("data".to_string()),
                },
                PxStep::Call {
                    name: "transform".to_string(),
                    params: serde_json::json!({"input": "$data"}),
                    output_var: Some("transformed".to_string()),
                },
            ],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L008").collect();
        assert!(diags.is_empty());
    }

    #[test]
    fn l008_multiple_shadows() {
        let mut doc = empty_doc();
        doc.procedures.push(make_proc(
            "pipeline",
            vec![
                PxStep::Call {
                    name: "step1".to_string(),
                    params: serde_json::json!({}),
                    output_var: Some("x".to_string()),
                },
                PxStep::Call {
                    name: "step2".to_string(),
                    params: serde_json::json!({}),
                    output_var: Some("x".to_string()),
                },
                PxStep::Call {
                    name: "step3".to_string(),
                    params: serde_json::json!({}),
                    output_var: Some("x".to_string()),
                },
            ],
        ));

        let diags: Vec<_> = lint(&doc).into_iter().filter(|d| d.code == "PX-L008").collect();
        // Two shadows: step 2 shadows step 1, step 3 shadows step 1
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].step_index, Some(1));
        assert_eq!(diags[1].step_index, Some(2));
    }
}
