//! Briefing action handlers — the pure classify/format assembler for
//! `morning-briefing.px`.
//!
//! # Why this exists (2026-07-08, TASK-2026-07-08-briefing-px STEP 2)
//!
//! The briefing procedure gathers three data sources by shelling out via
//! `run_command` (real [`ShellExecutor`](crate::shell_executor::ShellExecutor)):
//! ADO work items, open GitHub PRs, and CI run health. Each `run_command`
//! returns `{available, exit_code, stdout, stderr}` where `stdout` is a **string**.
//!
//! `.px` cannot parse a JSON (or line-structured) string into records — its only
//! string tools are interpolation pipe filters (`uppercase`/`trim`/`length`/…),
//! with no `from_json`. And `.px` object literals cannot resolve bare dotted refs
//! nor preserve numeric types (the documented limitation behind
//! [`worktask_actions::WorktaskActionHandler::make_task_record`]). So the
//! parse + classify + format step is done here, in **real** Rust, and returns a
//! native object the procedure delivers. This mirrors the `make_task_record`
//! pattern exactly: the *decision to gather / how to react to failure* stays in
//! `.px`; this action only shapes already-gathered data.
//!
//! # Safety (C-NOSTUB-001)
//!
//! Pure function over its inputs. No IO, no fabrication: every urgent/watch/
//! healthy/gap line is derived from the actual gathered text. When a source is
//! `available == false` (or its command failed / produced nothing), that source
//! becomes an explicit **gap** in the report — never a faked value, and the
//! caller still delivers (the `📝 Edit failed` side-quest class is structurally
//! impossible: this handler mutates no files).

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::px_adapter::AsyncActionHandler;
use pares_radix_praxis::px::executor::ExecutionError;

/// Actions handled by the briefing handler.
pub const BRIEFING_ACTIONS: &[&str] = &["assemble_briefing_report"];

/// Check whether an action name is handled by the briefing handler.
#[must_use]
pub fn is_briefing_action(action: &str) -> bool {
    BRIEFING_ACTIONS.contains(&action)
}

/// One classified work item / PR / CI signal in the briefing.
#[derive(Debug, Clone)]
struct Line {
    text: String,
}

/// The pure briefing assembler.
///
/// Stateless — construction is free.
#[derive(Debug, Default)]
pub struct BriefingActionHandler;

impl BriefingActionHandler {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Assemble the classified + formatted briefing from the three gather
    /// results.
    ///
    /// Params (each the object returned by `run_command`, or absent):
    /// - `ado` — `{available, stdout, ...}` from `briefing-ado.ps1`.
    /// - `github_prs` — `{available, stdout, ...}` from `gh pr list --json`.
    /// - `ci_health` — `{available, stdout, ...}` from `gh run list --json`.
    /// - `scope` — optional label (e.g. `"morning"`).
    ///
    /// Returns:
    /// `{ urgent[], watch[], healthy[], gaps[], report_text, counts, has_content }`.
    fn assemble_briefing_report(&self, params: &Value) -> Result<Value, ExecutionError> {
        let scope = params
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("morning");

        let mut urgent: Vec<Line> = Vec::new();
        let mut watch: Vec<Line> = Vec::new();
        let mut healthy: Vec<Line> = Vec::new();
        let mut gaps: Vec<String> = Vec::new();

        // ── ADO work items ──────────────────────────────────────────────────
        match gather_stdout(params.get("ado")) {
            GatherOutcome::Unavailable(reason) => {
                gaps.push(format!("ADO work items unavailable ({reason})"));
            }
            GatherOutcome::Ok(stdout) => {
                let trimmed = stdout.trim();
                if trimmed.is_empty() || trimmed == "NO_ACTIVE_ITEMS" {
                    healthy.push(Line {
                        text: "ADO: no active work items".to_string(),
                    });
                } else {
                    for raw in trimmed.lines() {
                        let line = raw.trim();
                        if line.is_empty() {
                            continue;
                        }
                        // Format: "<id> [<type>] P<n> <state> :: <title> | risk=.. | iter=.."
                        match parse_ado_priority(line) {
                            Some(0) | Some(1) => urgent.push(Line {
                                text: format!("🔴 {line}"),
                            }),
                            Some(2) => watch.push(Line {
                                text: format!("🟡 {line}"),
                            }),
                            _ => healthy.push(Line {
                                text: format!("• {line}"),
                            }),
                        }
                    }
                }
            }
        }

        // ── GitHub open PRs ─────────────────────────────────────────────────
        match gather_stdout(params.get("github_prs")) {
            GatherOutcome::Unavailable(reason) => {
                gaps.push(format!("Open PRs unavailable ({reason})"));
            }
            GatherOutcome::Ok(stdout) => match parse_json_array(&stdout) {
                Err(e) => gaps.push(format!("Open PRs unparseable ({e})")),
                Ok(arr) => {
                    if arr.is_empty() {
                        healthy.push(Line {
                            text: "PRs: none open".to_string(),
                        });
                    } else {
                        for pr in &arr {
                            let number = pr.get("number").and_then(Value::as_i64);
                            let title = pr
                                .get("title")
                                .and_then(Value::as_str)
                                .unwrap_or("(untitled)");
                            let draft = pr.get("isDraft").and_then(Value::as_bool).unwrap_or(false);
                            let review = pr
                                .get("reviewDecision")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            let num_str = number
                                .map(|n| format!("#{n}"))
                                .unwrap_or_else(|| "#?".to_string());
                            // A non-draft PR awaiting review is a watch item; a
                            // draft is informational; an approved PR is healthy.
                            if draft {
                                healthy.push(Line {
                                    text: format!("• PR {num_str} (draft): {title}"),
                                });
                            } else if review.eq_ignore_ascii_case("APPROVED") {
                                healthy.push(Line {
                                    text: format!("• PR {num_str} (approved): {title}"),
                                });
                            } else {
                                watch.push(Line {
                                    text: format!("🟡 PR {num_str} awaiting review: {title}"),
                                });
                            }
                        }
                    }
                }
            },
        }

        // ── CI run health ───────────────────────────────────────────────────
        match gather_stdout(params.get("ci_health")) {
            GatherOutcome::Unavailable(reason) => {
                gaps.push(format!("CI health unavailable ({reason})"));
            }
            GatherOutcome::Ok(stdout) => match parse_json_array(&stdout) {
                Err(e) => gaps.push(format!("CI health unparseable ({e})")),
                Ok(arr) => {
                    let mut failing = 0usize;
                    for run in &arr {
                        let status = run.get("status").and_then(Value::as_str).unwrap_or("");
                        let conclusion =
                            run.get("conclusion").and_then(Value::as_str).unwrap_or("");
                        let wf = run
                            .get("workflowName")
                            .and_then(Value::as_str)
                            .or_else(|| run.get("name").and_then(Value::as_str))
                            .unwrap_or("(workflow)");
                        let branch = run.get("headBranch").and_then(Value::as_str).unwrap_or("");
                        // Only completed runs carry a conclusion. A failure/
                        // timed_out/cancelled conclusion on a recent run is
                        // urgent; in-progress runs are watch; success is healthy.
                        if status == "completed" {
                            match conclusion {
                                "failure" | "timed_out" | "startup_failure" => {
                                    failing += 1;
                                    urgent.push(Line {
                                        text: format!("🔴 CI FAILING: {wf} on {branch}"),
                                    });
                                }
                                "cancelled" | "action_required" => watch.push(Line {
                                    text: format!("🟡 CI {conclusion}: {wf} on {branch}"),
                                }),
                                _ => {}
                            }
                        } else if !status.is_empty() {
                            watch.push(Line {
                                text: format!("🟡 CI {status}: {wf} on {branch}"),
                            });
                        }
                    }
                    if failing == 0
                        && arr
                            .iter()
                            .any(|r| r.get("conclusion").and_then(Value::as_str) == Some("success"))
                    {
                        healthy.push(Line {
                            text: "CI: recent runs green".to_string(),
                        });
                    }
                    if arr.is_empty() {
                        healthy.push(Line {
                            text: "CI: no recent runs".to_string(),
                        });
                    }
                }
            },
        }

        // ── Compose report text ─────────────────────────────────────────────
        let report_text = compose_report(scope, &urgent, &watch, &healthy, &gaps);
        let has_content =
            !urgent.is_empty() || !watch.is_empty() || !healthy.is_empty() || !gaps.is_empty();

        Ok(json!({
            "urgent": urgent.iter().map(|l| l.text.clone()).collect::<Vec<_>>(),
            "watch": watch.iter().map(|l| l.text.clone()).collect::<Vec<_>>(),
            "healthy": healthy.iter().map(|l| l.text.clone()).collect::<Vec<_>>(),
            "gaps": gaps,
            "counts": {
                "urgent": urgent.len(),
                "watch": watch.len(),
                "healthy": healthy.len(),
                "gaps": gaps_len(params),
            },
            "report_text": report_text,
            "has_content": has_content,
        }))
    }
}

#[async_trait]
impl AsyncActionHandler for BriefingActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "assemble_briefing_report" => self.assemble_briefing_report(params),
            other => Err(ExecutionError::UnknownAction(other.to_string())),
        }
    }
}

// ── free helpers (pure) ───────────────────────────────────────────────────────

/// Result of extracting a gather source's stdout.
enum GatherOutcome {
    /// The source produced stdout (may be empty).
    Ok(String),
    /// The source was unavailable — carries a human reason for the gap note.
    Unavailable(String),
}

/// Extract usable stdout from a `run_command` result object, or explain why not.
///
/// Honors the `{available:false, error}` contract the run-command boundary
/// returns on refusal/timeout, and treats a non-zero exit code as unavailable
/// (the underlying fetch failed) so it becomes an explicit gap.
fn gather_stdout(v: Option<&Value>) -> GatherOutcome {
    let Some(obj) = v else {
        return GatherOutcome::Unavailable("not gathered".to_string());
    };
    // Absent/false availability → gap with the provided error if any.
    let available = obj
        .get("available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !available {
        let err = obj
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("source unavailable");
        return GatherOutcome::Unavailable(err.to_string());
    }
    // Non-zero exit means the command ran but the fetch failed.
    if let Some(code) = obj.get("exit_code").and_then(Value::as_i64) {
        if code != 0 {
            let stderr = obj
                .get("stderr")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let reason = if stderr.is_empty() {
                format!("exit {code}")
            } else {
                format!("exit {code}: {}", first_line(stderr))
            };
            return GatherOutcome::Unavailable(reason);
        }
    }
    let stdout = obj
        .get("stdout")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    GatherOutcome::Ok(stdout)
}

/// Parse a `P<n>` priority token out of an ADO briefing line.
fn parse_ado_priority(line: &str) -> Option<u8> {
    for tok in line.split_whitespace() {
        if let Some(rest) = tok.strip_prefix('P') {
            if let Ok(n) = rest.parse::<u8>() {
                return Some(n);
            }
        }
    }
    None
}

/// Parse a JSON string into an array of objects, tolerating leading/trailing
/// whitespace. Returns a human error string on failure.
fn parse_json_array(s: &str) -> Result<Vec<Value>, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let parsed: Value = serde_json::from_str(trimmed).map_err(|e| e.to_string())?;
    match parsed {
        Value::Array(a) => Ok(a),
        other => Err(format!("expected JSON array, got {}", type_name(&other))),
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("").trim()
}

/// The number of gaps is derived inside the assembler; this recomputes it from
/// the raw params only for the `counts` object so callers get a stable count
/// without threading state. (Cheap; the report is small.)
fn gaps_len(params: &Value) -> usize {
    let mut n = 0;
    for key in ["ado", "github_prs", "ci_health"] {
        if let GatherOutcome::Unavailable(_) = gather_stdout(params.get(key)) {
            n += 1;
        }
    }
    n
}

/// Compose the human-readable briefing text from the classified buckets.
fn compose_report(
    scope: &str,
    urgent: &[Line],
    watch: &[Line],
    healthy: &[Line],
    gaps: &[String],
) -> String {
    let mut out = String::new();
    let title: String = match scope {
        "morning" => "☀️ Morning Briefing".to_string(),
        "afternoon" => "🌆 Afternoon Review".to_string(),
        other => format!("📋 Briefing ({other})"),
    };
    out.push_str(&title);
    out.push('\n');

    if !urgent.is_empty() {
        out.push_str("\n🔴 URGENT\n");
        for l in urgent {
            out.push_str(&l.text);
            out.push('\n');
        }
    }
    if !watch.is_empty() {
        out.push_str("\n🟡 WATCH\n");
        for l in watch {
            out.push_str(&l.text);
            out.push('\n');
        }
    }
    if !healthy.is_empty() {
        out.push_str("\n🟢 HEALTHY\n");
        for l in healthy {
            out.push_str(&l.text);
            out.push('\n');
        }
    }
    if !gaps.is_empty() {
        out.push_str("\n⚠️ GAPS (data unavailable — noted, not faked)\n");
        for g in gaps {
            out.push_str("• ");
            out.push_str(g);
            out.push('\n');
        }
    }
    if urgent.is_empty() && watch.is_empty() && healthy.is_empty() && gaps.is_empty() {
        out.push_str("\n(no signals)\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(stdout: &str) -> Value {
        json!({"available": true, "exit_code": 0, "stdout": stdout, "stderr": ""})
    }

    #[test]
    fn gate_matches_only_briefing_action() {
        assert!(is_briefing_action("assemble_briefing_report"));
        assert!(!is_briefing_action("run_command"));
    }

    #[test]
    fn classifies_ado_priority_into_urgent_watch_healthy() {
        let h = BriefingActionHandler::new();
        let ado = ok("111 [Bug] P1 Active :: Fix auth | risk=High | iter=S1\n\
                      222 [Task] P2 Active :: Docs | risk=Low | iter=S1\n\
                      333 [Task] P3 New :: Cleanup | risk=Low | iter=S1");
        let out = h
            .assemble_briefing_report(&json!({"ado": ado, "scope": "morning"}))
            .unwrap();
        assert_eq!(out["urgent"].as_array().unwrap().len(), 1);
        assert_eq!(out["watch"].as_array().unwrap().len(), 1);
        // 333 (P3) healthy; github_prs + ci_health absent → 2 gaps.
        assert!(out["healthy"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("Cleanup")));
        assert_eq!(out["gaps"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn ado_no_active_items_is_healthy_not_gap() {
        let h = BriefingActionHandler::new();
        let out = h
            .assemble_briefing_report(&json!({"ado": ok("NO_ACTIVE_ITEMS")}))
            .unwrap();
        assert!(out["healthy"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("no active work items")));
    }

    #[test]
    fn unavailable_source_becomes_gap_and_still_delivers() {
        let h = BriefingActionHandler::new();
        let out = h
            .assemble_briefing_report(&json!({
                "ado": {"available": false, "error": "az login required"},
            }))
            .unwrap();
        let gaps = out["gaps"].as_array().unwrap();
        assert!(gaps
            .iter()
            .any(|g| g.as_str().unwrap().contains("az login required")));
        // has_content true because there are gaps → report still delivered.
        assert_eq!(out["has_content"], json!(true));
        assert!(out["report_text"].as_str().unwrap().contains("GAPS"));
    }

    #[test]
    fn nonzero_exit_is_a_gap_with_stderr() {
        let h = BriefingActionHandler::new();
        let bad = json!({"available": true, "exit_code": 1, "stdout": "", "stderr": "boom\nmore"});
        let out = h
            .assemble_briefing_report(&json!({"github_prs": bad}))
            .unwrap();
        assert!(out["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g.as_str().unwrap().contains("boom")));
    }

    #[test]
    fn parses_open_prs_json_awaiting_review_as_watch() {
        let h = BriefingActionHandler::new();
        let prs = ok(
            r#"[{"number":42,"title":"Add feature","isDraft":false,"reviewDecision":"REVIEW_REQUIRED","mergeable":"MERGEABLE","createdAt":"2026-07-01T00:00:00Z"}]"#,
        );
        let out = h
            .assemble_briefing_report(&json!({"github_prs": prs}))
            .unwrap();
        assert!(out["watch"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("#42 awaiting review")));
    }

    #[test]
    fn ci_failure_is_urgent_success_is_healthy() {
        let h = BriefingActionHandler::new();
        let runs = ok(
            r#"[{"status":"completed","conclusion":"failure","workflowName":"CI","headBranch":"main"},{"status":"completed","conclusion":"success","workflowName":"CI","headBranch":"main"}]"#,
        );
        let out = h
            .assemble_briefing_report(&json!({"ci_health": runs}))
            .unwrap();
        assert!(out["urgent"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("CI FAILING")));
    }

    #[test]
    fn empty_report_when_nothing_gathered() {
        let h = BriefingActionHandler::new();
        // All three sources absent → 3 gaps (not truly empty), report notes them.
        let out = h.assemble_briefing_report(&json!({})).unwrap();
        assert_eq!(out["gaps"].as_array().unwrap().len(), 3);
        assert_eq!(out["has_content"], json!(true));
    }

    #[test]
    fn unknown_action_errors() {
        let h = BriefingActionHandler::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(h.call("nope", &json!({})))
            .expect_err("unknown action must error");
        matches!(err, ExecutionError::UnknownAction(_));
    }
}
