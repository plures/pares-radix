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
}

/// Lint a single step (recursing into nested structures).
fn lint_step(step: &PxStep, proc_name: &str, idx: usize, diags: &mut Vec<LintDiagnostic>) {
    match step {
        PxStep::Match { arms } => {
            lint_match_exhaustiveness(arms, proc_name, idx, diags);
            lint_match_unreachable(arms, proc_name, idx, diags);
        }
        PxStep::Loop { steps, .. } => {
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
            scenarios: vec![],
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
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "PX-L002");
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
}
