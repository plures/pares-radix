//! Repo-health action handlers - the classify + persist boundary for
//! `repo-health-sweep.px` (epic:ci-org-health-monitor-implementation, slice 1).
//!
//! # Why this exists
//!
//! `repo-health-sweep.px` gathers CI-run and open-PR data for one repo via the
//! governed `run_command` action (real `gh` subprocess, same boundary as
//! `morning-briefing.px`). `.px` cannot parse the returned JSON strings itself
//! (no `from_json` filter), so classification happens here in real Rust,
//! mirroring [`BriefingActionHandler`](crate::spine::briefing_actions).
//!
//! Two actions:
//! - `classify_repo_health` - pure function, gathered JSON in, anomaly list
//!   out. No IO, no fabrication (C-NOSTUB-001): every anomaly is derived from
//!   real gathered data, and an unavailable/failed source becomes an explicit
//!   `gap` anomaly rather than being silently dropped.
//! - `record_health_anomaly` - the ONLY side effect. Persists one anomaly as a
//!   real row in the shared [`StateStore`](crate::state::StateStore) (PluresDB
//!   in production) under key `health_anomaly:<repo>:<anomaly_id>`, so the
//!   whole table is enumerable via `keys_with_prefix("health_anomaly:")`.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::px_adapter::AsyncActionHandler;
use crate::state::StateStore;
use pares_radix_praxis::px::executor::ExecutionError;

/// Actions handled by the repo-health handler.
pub const REPO_HEALTH_ACTIONS: &[&str] = &["classify_repo_health", "record_health_anomaly"];

/// Check whether an action name is handled by the repo-health handler.
#[must_use]
pub fn is_repo_health_action(action: &str) -> bool {
    REPO_HEALTH_ACTIONS.contains(&action)
}

/// A single classified anomaly, ready to persist.
#[derive(Debug, Clone)]
struct Anomaly {
    kind: &'static str,
    severity: &'static str,
    detail: String,
}

/// Repo-health action handler: classify gathered CI/PR data into anomalies,
/// and persist each anomaly as a durable `health_anomaly:<repo>:<id>` row.
pub struct RepoHealthActionHandler {
    state: Arc<dyn StateStore>,
}

impl RepoHealthActionHandler {
    #[must_use]
    pub fn new(state: Arc<dyn StateStore>) -> Self {
        Self { state }
    }

    /// Classify gathered CI-run + open-PR data into anomaly records.
    ///
    /// Params:
    /// - `repo` - `"<owner>/<name>"`.
    /// - `ci_health` - `{available, exit_code, stdout, ...}` from
    ///   `gh run list --json ...`.
    /// - `open_prs` - `{available, exit_code, stdout, ...}` from
    ///   `gh pr list --json ...`.
    ///
    /// Returns a JSON array of `{kind, severity, detail, detected_at}`.
    fn classify(&self, params: &Value) -> Result<Value, ExecutionError> {
        let repo = params.get("repo").and_then(Value::as_str).unwrap_or("");
        let now = now_unix();
        let mut anomalies: Vec<Anomaly> = Vec::new();

        // ── CI runs ──────────────────────────────────────────────────────────
        match gather_stdout(params.get("ci_health")) {
            GatherOutcome::Unavailable(reason) => {
                anomalies.push(Anomaly {
                    kind: "gap",
                    severity: "warning",
                    detail: format!("CI data unavailable for {repo}: {reason}"),
                });
            }
            GatherOutcome::Ok(stdout) => match parse_json_array(&stdout) {
                Err(e) => {
                    anomalies.push(Anomaly {
                        kind: "gap",
                        severity: "warning",
                        detail: format!("CI data unparseable for {repo}: {e}"),
                    });
                }
                Ok(runs) => {
                    for run in &runs {
                        let status = run.get("status").and_then(Value::as_str).unwrap_or("");
                        let conclusion =
                            run.get("conclusion").and_then(Value::as_str).unwrap_or("");
                        let workflow = run
                            .get("workflowName")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown workflow");
                        let branch = run
                            .get("headBranch")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown branch");
                        match conclusion {
                            "failure" | "startup_failure" => anomalies.push(Anomaly {
                                kind: "ci_failing",
                                severity: "error",
                                detail: format!("{workflow} failing on {branch}"),
                            }),
                            "timed_out" => anomalies.push(Anomaly {
                                kind: "ci_timeout",
                                severity: "error",
                                detail: format!("{workflow} timed out on {branch}"),
                            }),
                            "" if status == "in_progress" || status == "queued" => {
                                // Not an anomaly - still running.
                            }
                            _ => {}
                        }
                    }
                }
            },
        }

        // ── Open PRs (stale = open > 14 days) ───────────────────────────────
        match gather_stdout(params.get("open_prs")) {
            GatherOutcome::Unavailable(reason) => {
                anomalies.push(Anomaly {
                    kind: "gap",
                    severity: "warning",
                    detail: format!("PR data unavailable for {repo}: {reason}"),
                });
            }
            GatherOutcome::Ok(stdout) => match parse_json_array(&stdout) {
                Err(e) => {
                    anomalies.push(Anomaly {
                        kind: "gap",
                        severity: "warning",
                        detail: format!("PR data unparseable for {repo}: {e}"),
                    });
                }
                Ok(prs) => {
                    for pr in &prs {
                        let number = pr.get("number").and_then(Value::as_i64).unwrap_or(0);
                        let title = pr.get("title").and_then(Value::as_str).unwrap_or("");
                        let created = pr.get("createdAt").and_then(Value::as_str).unwrap_or("");
                        if let Some(age_days) = age_days_since(created, now) {
                            if age_days >= 14 {
                                anomalies.push(Anomaly {
                                    kind: "stale_pr",
                                    severity: "warning",
                                    detail: format!(
                                        "PR #{number} open {age_days}d: {title}"
                                    ),
                                });
                            }
                        }
                    }
                }
            },
        }

        Ok(Value::Array(
            anomalies
                .into_iter()
                .map(|a| {
                    json!({
                        "kind": a.kind,
                        "severity": a.severity,
                        "detail": a.detail,
                        "detected_at": now,
                    })
                })
                .collect(),
        ))
    }

    /// Persist one anomaly as a durable `health_anomaly:<repo>:<id>` row.
    async fn record(&self, params: &Value) -> Result<Value, ExecutionError> {
        let repo = params
            .get("repo")
            .and_then(Value::as_str)
            .ok_or_else(|| ExecutionError::ActionFailed {
                action: "record_health_anomaly".into(),
                message: "missing repo".into(),
            })?;
        let anomaly =
            params
                .get("anomaly")
                .cloned()
                .ok_or_else(|| ExecutionError::ActionFailed {
                    action: "record_health_anomaly".into(),
                    message: "missing anomaly".into(),
                })?;

        let kind = anomaly.get("kind").and_then(Value::as_str).unwrap_or("unknown");
        let detail_hash = simple_hash(
            anomaly
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or(""),
        );
        let key = format!("health_anomaly:{repo}:{kind}:{detail_hash}");

        let record = json!({
            "repo": repo,
            "kind": kind,
            "severity": anomaly.get("severity").cloned().unwrap_or(json!("warning")),
            "detail": anomaly.get("detail").cloned().unwrap_or(json!("")),
            "detected_at": anomaly.get("detected_at").cloned().unwrap_or(json!(now_unix())),
        });

        self.state.set(&key, record.clone()).await;
        Ok(record)
    }
}

#[async_trait]
impl AsyncActionHandler for RepoHealthActionHandler {
    async fn call(&self, name: &str, params: &Value) -> Result<Value, ExecutionError> {
        match name {
            "classify_repo_health" => self.classify(params),
            "record_health_anomaly" => self.record(params).await,
            other => Err(ExecutionError::UnknownAction(other.to_string())),
        }
    }
}

// ── free helpers (pure) ──────────────────────────────────────────────────────

enum GatherOutcome {
    Ok(String),
    Unavailable(String),
}

fn gather_stdout(v: Option<&Value>) -> GatherOutcome {
    let Some(obj) = v else {
        return GatherOutcome::Unavailable("not gathered".to_string());
    };
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
                format!("exit {code}: {}", stderr.lines().next().unwrap_or(stderr))
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

fn parse_json_array(s: &str) -> Result<Vec<Value>, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let v: Value = serde_json::from_str(trimmed).map_err(|e| e.to_string())?;
    match v {
        Value::Array(arr) => Ok(arr),
        other => Err(format!("expected JSON array, got {other}")),
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parse an ISO-8601 `createdAt` timestamp (as returned by `gh pr list --json
/// createdAt`, e.g. `2026-07-01T12:00:00Z`) and return whole days since then.
/// Returns `None` if the timestamp cannot be parsed (never fabricates an age).
fn age_days_since(created_at: &str, now: i64) -> Option<i64> {
    let created_unix = parse_iso8601_to_unix(created_at)?;
    Some(((now - created_unix).max(0)) / 86400)
}

/// Minimal ISO-8601 `YYYY-MM-DDTHH:MM:SSZ` parser (UTC only, no external date
/// crate dependency). Returns `None` on any malformed input.
fn parse_iso8601_to_unix(s: &str) -> Option<i64> {
    let s = s.trim().strip_suffix('Z').unwrap_or(s.trim());
    let (date, time) = s.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = time_parts
        .next()
        .and_then(|s| s.split('.').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Days since epoch (1970-01-01) via civil_from_days algorithm (Howard
    // Hinnant's date algorithms - well-known, no external crate needed).
    let days = days_from_civil(year, month, day);
    Some(days * 86400 + hour * 3600 + minute * 60 + second)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Deterministic short hash for dedup keying (not cryptographic - just needs
/// to be stable across identical `detail` strings so re-sweeps don't create
/// duplicate rows for the same anomaly).
fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 1469598103934665603; // FNV-1a offset basis
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::InMemoryStateStore;

    fn handler() -> RepoHealthActionHandler {
        RepoHealthActionHandler::new(Arc::new(InMemoryStateStore::new()))
    }

    fn ok_result(stdout: &str) -> Value {
        json!({"available": true, "exit_code": 0, "stdout": stdout, "stderr": ""})
    }

    #[test]
    fn classify_detects_ci_failing() {
        let h = handler();
        let ci = ok_result(
            r#"[{"databaseId":1,"name":"ci","status":"completed","conclusion":"failure","workflowName":"CI","headBranch":"main","createdAt":"2026-07-20T00:00:00Z"}]"#,
        );
        let prs = ok_result("[]");
        let params = json!({"repo": "plures/pares-radix", "ci_health": ci, "open_prs": prs});
        let anomalies = h.classify(&params).unwrap();
        let arr = anomalies.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["kind"], json!("ci_failing"));
        assert_eq!(arr[0]["severity"], json!("error"));
        assert!(arr[0]["detail"].as_str().unwrap().contains("CI failing on main"));
    }

    #[test]
    fn classify_detects_ci_timeout() {
        let h = handler();
        let ci = ok_result(
            r#"[{"databaseId":1,"name":"ci","status":"completed","conclusion":"timed_out","workflowName":"CI","headBranch":"main"}]"#,
        );
        let prs = ok_result("[]");
        let params = json!({"repo": "plures/pares-radix", "ci_health": ci, "open_prs": prs});
        let anomalies = h.classify(&params).unwrap();
        let arr = anomalies.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["kind"], json!("ci_timeout"));
    }

    #[test]
    fn classify_detects_stale_pr() {
        let h = handler();
        let ci = ok_result("[]");
        // Fixed far-past date guarantees >=14 days old regardless of test run time.
        let prs = ok_result(
            r#"[{"number":42,"title":"old pr","createdAt":"2020-01-01T00:00:00Z","isDraft":false}]"#,
        );
        let params = json!({"repo": "plures/pares-radix", "ci_health": ci, "open_prs": prs});
        let anomalies = h.classify(&params).unwrap();
        let arr = anomalies.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["kind"], json!("stale_pr"));
        assert!(arr[0]["detail"].as_str().unwrap().contains("#42"));
    }

    #[test]
    fn classify_records_gap_on_unavailable_source() {
        let h = handler();
        let ci = json!({"available": false, "error": "gh timed out"});
        let prs = ok_result("[]");
        let params = json!({"repo": "plures/pares-radix", "ci_health": ci, "open_prs": prs});
        let anomalies = h.classify(&params).unwrap();
        let arr = anomalies.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["kind"], json!("gap"));
        assert!(arr[0]["detail"].as_str().unwrap().contains("gh timed out"));
    }

    #[test]
    fn classify_no_anomalies_on_clean_repo() {
        let h = handler();
        let ci = ok_result(
            r#"[{"databaseId":1,"name":"ci","status":"completed","conclusion":"success","workflowName":"CI","headBranch":"main"}]"#,
        );
        let prs = ok_result("[]");
        let params = json!({"repo": "plures/pares-radix", "ci_health": ci, "open_prs": prs});
        let anomalies = h.classify(&params).unwrap();
        assert_eq!(anomalies.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn record_persists_a_real_row_readable_via_prefix_scan() {
        let state = Arc::new(InMemoryStateStore::new());
        let h = RepoHealthActionHandler::new(Arc::clone(&state) as Arc<dyn StateStore>);
        let anomaly = json!({
            "kind": "ci_failing",
            "severity": "error",
            "detail": "CI failing on main",
            "detected_at": 1_700_000_000i64,
        });
        let params = json!({"repo": "plures/pares-radix", "anomaly": anomaly});
        let written = h.record(&params).await.unwrap();
        assert_eq!(written["repo"], json!("plures/pares-radix"));
        assert_eq!(written["kind"], json!("ci_failing"));

        // Prove it's a REAL durable row: enumerate via keys_with_prefix and
        // read it back through the StateStore directly (not the handler).
        let keys = state.keys_with_prefix("health_anomaly:plures/pares-radix:").await;
        assert_eq!(keys.len(), 1, "expected exactly one persisted anomaly row");
        let read_back = state.get(&keys[0]).await.expect("row must be readable");
        assert_eq!(read_back["detail"], json!("CI failing on main"));
    }

    #[tokio::test]
    async fn record_dedupes_identical_anomaly_on_resweep() {
        let state = Arc::new(InMemoryStateStore::new());
        let h = RepoHealthActionHandler::new(Arc::clone(&state) as Arc<dyn StateStore>);
        let anomaly = json!({
            "kind": "stale_pr",
            "severity": "warning",
            "detail": "PR #7 open 20d: fix thing",
            "detected_at": 1_700_000_000i64,
        });
        let params = json!({"repo": "plures/pares-radix", "anomaly": anomaly.clone()});
        h.record(&params).await.unwrap();
        h.record(&params).await.unwrap();
        let keys = state.keys_with_prefix("health_anomaly:").await;
        assert_eq!(keys.len(), 1, "identical anomaly must not duplicate the row");
    }

    #[test]
    fn full_px_procedure_parses_and_compiles() {
        // Guards the .px file itself, mirroring briefing_px_loads.rs.
        use crate::px_adapter::load_px_procedures;
        use std::sync::Arc as StdArc;

        struct Noop;
        #[async_trait::async_trait]
        impl AsyncActionHandler for Noop {
            async fn call(&self, name: &str, _p: &Value) -> Result<Value, ExecutionError> {
                Err(ExecutionError::UnknownAction(name.to_string()))
            }
        }

        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("praxis")
            .join("procedures")
            .join("repo-health-sweep.px");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let handler: StdArc<dyn AsyncActionHandler> = StdArc::new(Noop);
        let adapters = load_px_procedures(&source, handler)
            .unwrap_or_else(|e| panic!("repo-health-sweep.px must parse+compile: {e}"));
        assert_eq!(adapters.len(), 1, "expected exactly one procedure to compile");
    }
}
