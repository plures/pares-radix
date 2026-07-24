//! Chronos-watcher — the RSI **detection** boundary (observe / propose / notify).
//!
//! # What this is (and deliberately is NOT)
//!
//! This is the Phase 3b *detection* half of the recursive-self-improvement (RSI)
//! loop. It **observes** the chronos timeline for the loop's trigger signals,
//! **proposes** an improvement (by recording an `ImprovementProposal` to the
//! PluresDB-backed timeline), and **notifies** (records a notification entry the
//! host can route to a human). It is strictly *observe-only*:
//!
//! * It performs **no enforcement** — it never calls `PraxisWriteGate::add_constraint`
//!   or any other mutation that changes what the system enforces.
//! * It never applies a `.px` change, registers a procedure, or modifies the loop.
//! * Everything it emits is a *proposal* or a *notification* for human review.
//!
//! This matches the System-Cohesion Phase 3b design boundary: the loop OBSERVES +
//! PROPOSES + NOTIFIES; the real enforcement flip is **Phase 4** and remains gated
//! on the (currently non-existent) `compile_nl` NL→constraint encode arrow. Wiring
//! detection now, with no enforcement, is safe by construction.
//!
//! # Trigger signals it watches
//!
//! Reading `ChronosTimeline::recent(..)` newest-first, it counts, per improvement
//! *target*:
//!
//! * [`ChronosAction::OutcomeRecorded`] entries whose rationale marks a **user
//!   correction** (the strongest "something is wrong here" signal), and
//! * performance-signal / task-completion writes that carry a **regression** or
//!   **repeated-retry** marker.
//!
//! When the observed corrections for a single target reach a threshold (default 3,
//! matching the RSI `improvement_needs_evidence` rail — improvements need ≥3
//! observations), it emits **one** proposal + notification for that target. It does
//! not propose for a target it has already proposed for in the scanned window
//! (dedupe), so repeated scans do not spam.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::chronos::{ChronosAction, ChronosEntry, ChronosTimeline};
use pluresdb::CrdtStore;

/// Actor label attributed to entries this watcher records.
pub const WATCHER_ACTOR: &str = "rsi:chronos-watcher";

/// Chronos key prefix under which improvement proposals are persisted.
pub const PROPOSAL_KEY_PREFIX: &str = "rsi:proposal:";
/// Chronos key prefix under which notifications are persisted.
pub const NOTIFICATION_KEY_PREFIX: &str = "rsi:notification:";

/// Default number of correction observations required before proposing.
///
/// Mirrors the RSI `improvement_needs_evidence` constraint (evidence ≥ 3).
pub const DEFAULT_EVIDENCE_THRESHOLD: usize = 3;

/// A proposed improvement the watcher surfaces for human review.
///
/// This is a *proposal*, never an applied change. It records what was observed
/// and what is suggested; a human (or a later, human-gated phase) decides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImprovementProposal {
    /// The improvement target (e.g. a procedure name or a constraint id) the
    /// observed signals cluster around.
    pub target: String,
    /// Why this is being proposed, in one line.
    pub rationale: String,
    /// The chronos entry ids that constitute the supporting evidence.
    pub evidence_entry_ids: Vec<String>,
    /// How many supporting observations were seen.
    pub observation_count: usize,
    /// Wall-clock seconds when the proposal was generated.
    pub proposed_at: u64,
    /// Always `false` in Phase 3b. Present so the persisted shape is stable when
    /// a later, human-gated phase records approval — the watcher never sets it.
    pub approved: bool,
}

/// A notification the watcher emits alongside a proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatcherNotification {
    /// The proposal target this notification is about.
    pub target: String,
    /// Human-readable message.
    pub message: String,
    /// Severity is always informational — a proposal is not an incident.
    pub level: String,
    /// Seconds when emitted.
    pub emitted_at: u64,
}

/// The outcome of a single observation pass.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchOutcome {
    /// Proposals newly recorded this pass.
    pub proposals: Vec<ImprovementProposal>,
    /// Notifications newly emitted this pass.
    pub notifications: Vec<WatcherNotification>,
    /// Number of chronos entries scanned.
    pub scanned: usize,
}

/// Observe-only RSI detection component over a [`ChronosTimeline`].
///
/// Holds a [`ChronosTimeline`] (the append-only audit log it scans + writes
/// proposal/notification *audit* entries to) and an [`Arc<CrdtStore>`] (the
/// PluresDB-backed store where the full, retrievable proposal/notification
/// payloads live, per C-PLURES-003). It never holds a write-gate: by
/// construction it cannot mutate enforced state.
pub struct ChronosWatcher {
    timeline: Arc<ChronosTimeline>,
    store: Arc<CrdtStore>,
    evidence_threshold: usize,
}

impl ChronosWatcher {
    /// Create a watcher over the given timeline + store with the default
    /// evidence threshold ([`DEFAULT_EVIDENCE_THRESHOLD`]).
    pub fn new(timeline: Arc<ChronosTimeline>, store: Arc<CrdtStore>) -> Self {
        Self {
            timeline,
            store,
            evidence_threshold: DEFAULT_EVIDENCE_THRESHOLD,
        }
    }

    /// Create a watcher with an explicit evidence threshold (must be ≥ 1).
    pub fn with_threshold(
        timeline: Arc<ChronosTimeline>,
        store: Arc<CrdtStore>,
        evidence_threshold: usize,
    ) -> Self {
        Self {
            timeline,
            store,
            evidence_threshold: evidence_threshold.max(1),
        }
    }

    /// Return true if a chronos entry is an RSI "something needs improvement"
    /// signal for the purposes of detection, yielding the improvement target.
    ///
    /// The two signal shapes we recognise (distinct, non-overlapping intent):
    /// * an [`ChronosAction::OutcomeRecorded`] entry whose rationale mentions a
    ///   user **correction/rejection** (the strongest "this output was wrong"
    ///   signal), OR
    /// * **any** entry whose rationale explicitly marks a **regression**,
    ///   **repeated retry**, or **bottleneck** (performance-signal writers
    ///   annotate these regardless of action kind).
    fn signal_target(entry: &ChronosEntry) -> Option<String> {
        let rationale = entry.rationale.as_deref().unwrap_or("");
        let lc = rationale.to_ascii_lowercase();

        // (a) A user correction/rejection, but only when it is an OutcomeRecorded
        //     entry (that action is what carries user acceptance/correction).
        let is_correction_outcome = matches!(entry.action, ChronosAction::OutcomeRecorded)
            && (lc.contains("correction") || lc.contains("corrected") || lc.contains("rejected"));

        // (b) An explicit performance problem marker on any action kind.
        let is_performance_problem =
            lc.contains("regression") || lc.contains("repeated retry") || lc.contains("bottleneck");

        if is_correction_outcome || is_performance_problem {
            // The improvement target is the entry's data key (which the RSI
            // producers set to the procedure/constraint the signal is about).
            Some(entry.key.clone())
        } else {
            None
        }
    }

    /// Whether a proposal already exists (persisted) for `target`.
    ///
    /// Dedupe guard: prevents re-proposing on every pass. Reads the persisted
    /// proposal payload for the target from the PluresDB store.
    fn already_proposed(&self, target: &str) -> bool {
        let key = format!("{PROPOSAL_KEY_PREFIX}{target}");
        self.store.get(&key).is_some()
    }

    /// Run one observation pass over the most recent `limit` chronos entries.
    ///
    /// For each target that accrues ≥ `evidence_threshold` problem signals and
    /// has not already been proposed, records **one** [`ImprovementProposal`]
    /// and one [`WatcherNotification`] to the timeline (PluresDB-backed) and
    /// returns them. Records nothing and returns empty vectors when no target
    /// crosses the threshold.
    ///
    /// This method never enforces anything; it only reads the timeline and
    /// appends proposal/notification entries for human review.
    pub fn observe_once(&self, limit: usize) -> WatchOutcome {
        let entries = self.timeline.recent(limit);
        let scanned = entries.len();

        // Group supporting evidence entry-ids by target, newest-first order.
        let mut by_target: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for e in &entries {
            if let Some(target) = Self::signal_target(e) {
                by_target.entry(target).or_default().push(e.id.clone());
            }
        }

        let now = now_secs();
        let mut outcome = WatchOutcome {
            scanned,
            ..Default::default()
        };

        for (target, evidence_ids) in by_target {
            if evidence_ids.len() < self.evidence_threshold {
                continue;
            }
            if self.already_proposed(&target) {
                continue;
            }

            let proposal = ImprovementProposal {
                target: target.clone(),
                rationale: format!(
                    "Observed {} correction/regression signals for '{}' \
                     (≥ threshold {}). Proposing review; NOT auto-applied.",
                    evidence_ids.len(),
                    target,
                    self.evidence_threshold
                ),
                evidence_entry_ids: evidence_ids.clone(),
                observation_count: evidence_ids.len(),
                proposed_at: now,
                approved: false,
            };

            // Persist the full proposal payload to the PluresDB store (the
            // retrievable source of truth, C-PLURES-003) AND append an audit
            // entry to the chronos timeline. Observe-only: a proposal record,
            // never an enforcement change.
            let proposal_key = format!("{PROPOSAL_KEY_PREFIX}{target}");
            self.store.put(
                proposal_key.clone(),
                WATCHER_ACTOR,
                serde_json::to_value(&proposal).expect("ImprovementProposal serializes"),
            );
            let proposal_entry = self.timeline.build_entry(
                &proposal_key,
                WATCHER_ACTOR,
                // Create: this is a new proposal record, not a data mutation of
                // any enforced state.
                ChronosAction::Create,
                &serde_json::to_value(&proposal).expect("ImprovementProposal serializes"),
                Vec::new(),
                Some(proposal.rationale.clone()),
            );
            self.timeline.record(&proposal_entry);

            let notification = WatcherNotification {
                target: target.clone(),
                message: format!(
                    "RSI detection: '{}' accrued {} problem signals; \
                     an improvement proposal was recorded for human review.",
                    target,
                    evidence_ids.len()
                ),
                level: "info".to_string(),
                emitted_at: now,
            };
            let notification_key = format!("{NOTIFICATION_KEY_PREFIX}{target}");
            self.store.put(
                notification_key.clone(),
                WATCHER_ACTOR,
                serde_json::to_value(&notification).expect("WatcherNotification serializes"),
            );
            let notification_entry = self.timeline.build_entry(
                &notification_key,
                WATCHER_ACTOR,
                ChronosAction::Create,
                &serde_json::to_value(&notification).expect("WatcherNotification serializes"),
                Vec::new(),
                Some(notification.message.clone()),
            );
            self.timeline.record(&notification_entry);

            tracing::info!(
                target = %target,
                observations = evidence_ids.len(),
                "RSI chronos-watcher recorded an improvement proposal + notification (observe-only)"
            );

            outcome.proposals.push(proposal);
            outcome.notifications.push(notification);
        }

        outcome
    }

    /// Read back the persisted proposal for `target`, if any.
    ///
    /// Returns the full [`ImprovementProposal`] payload from the PluresDB store
    /// (the retrievable source of truth), not a reconstruction.
    pub fn proposal_for(&self, target: &str) -> Option<ImprovementProposal> {
        let key = format!("{PROPOSAL_KEY_PREFIX}{target}");
        let record = self.store.get(&key)?;
        serde_json::from_value(record.data).ok()
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chronos::ChronosTimeline;
    use pluresdb::CrdtStore;
    use serde_json::json;

    fn make_env() -> (Arc<ChronosTimeline>, Arc<CrdtStore>) {
        let store = Arc::new(CrdtStore::default());
        let timeline = Arc::new(ChronosTimeline::new(Arc::clone(&store)));
        (timeline, store)
    }

    /// Helper: record an OutcomeRecorded correction signal for `target`.
    fn record_correction(timeline: &ChronosTimeline, target: &str) {
        let entry = timeline.build_entry(
            target,
            "user",
            ChronosAction::OutcomeRecorded,
            &json!({"kind": "correction"}),
            Vec::new(),
            Some(format!("user correction on {target}")),
        );
        timeline.record(&entry);
    }

    #[test]
    fn below_threshold_produces_no_proposal() {
        let (timeline, store) = make_env();
        // Only 2 signals — below the default threshold of 3.
        record_correction(&timeline, "procedure:foo");
        record_correction(&timeline, "procedure:foo");

        let watcher = ChronosWatcher::new(Arc::clone(&timeline), store);
        let out = watcher.observe_once(100);
        assert!(out.proposals.is_empty(), "must not propose below threshold");
        assert!(out.notifications.is_empty());
        assert!(out.scanned >= 2);
    }

    #[test]
    fn threshold_reached_records_proposal_and_notification() {
        let (timeline, store) = make_env();
        for _ in 0..3 {
            record_correction(&timeline, "procedure:bar");
        }

        let watcher = ChronosWatcher::new(Arc::clone(&timeline), store);
        let out = watcher.observe_once(100);

        assert_eq!(
            out.proposals.len(),
            1,
            "exactly one proposal for the target"
        );
        assert_eq!(out.notifications.len(), 1);
        let p = &out.proposals[0];
        assert_eq!(p.target, "procedure:bar");
        assert_eq!(p.observation_count, 3);
        assert_eq!(p.evidence_entry_ids.len(), 3);
        assert!(!p.approved, "watcher never auto-approves (observe-only)");
        assert_eq!(out.notifications[0].level, "info");
    }

    #[test]
    fn proposal_is_persisted_and_deduped_across_passes() {
        let (timeline, store) = make_env();
        for _ in 0..4 {
            record_correction(&timeline, "constraint:baz");
        }

        let watcher = ChronosWatcher::new(Arc::clone(&timeline), store);

        // First pass proposes.
        let first = watcher.observe_once(100);
        assert_eq!(first.proposals.len(), 1);

        // The FULL proposal payload was persisted (readable back from the store).
        assert!(watcher.already_proposed("constraint:baz"));
        let stored = watcher
            .proposal_for("constraint:baz")
            .expect("proposal must be persisted to the store");
        assert_eq!(stored.target, "constraint:baz");
        assert_eq!(stored.observation_count, 4);
        assert_eq!(stored.evidence_entry_ids.len(), 4);

        // Second pass over the same signals must NOT re-propose (dedupe).
        let second = watcher.observe_once(100);
        assert!(
            second.proposals.is_empty(),
            "must not re-propose an already-proposed target"
        );
    }

    #[test]
    fn distinct_targets_each_get_their_own_proposal() {
        let (timeline, store) = make_env();
        for _ in 0..3 {
            record_correction(&timeline, "procedure:alpha");
        }
        for _ in 0..3 {
            record_correction(&timeline, "procedure:beta");
        }

        let watcher = ChronosWatcher::new(Arc::clone(&timeline), store);
        let out = watcher.observe_once(200);
        assert_eq!(out.proposals.len(), 2, "one proposal per distinct target");
        let targets: std::collections::BTreeSet<_> =
            out.proposals.iter().map(|p| p.target.clone()).collect();
        assert!(targets.contains("procedure:alpha"));
        assert!(targets.contains("procedure:beta"));
    }

    #[test]
    fn non_problem_signals_are_ignored() {
        let (timeline, store) = make_env();
        // A benign create with no problem rationale should not be a signal.
        for _ in 0..5 {
            let e = timeline.build_entry(
                "procedure:healthy",
                "system",
                ChronosAction::Create,
                &json!({"ok": true}),
                Vec::new(),
                Some("routine write".to_string()),
            );
            timeline.record(&e);
        }
        let watcher = ChronosWatcher::new(Arc::clone(&timeline), store);
        let out = watcher.observe_once(100);
        assert!(out.proposals.is_empty(), "benign signals must not propose");
    }

    #[test]
    fn observe_once_never_touches_enforcement() {
        // This test documents the observe-only contract: the watcher only needs
        // a timeline + store, never a write-gate. There is no code path here
        // that can mutate enforced state. (Compile-time proof: ChronosWatcher
        // holds only an Arc<ChronosTimeline> + Arc<CrdtStore>; it has no
        // PraxisWriteGate handle at all.)
        let (timeline, store) = make_env();
        for _ in 0..3 {
            record_correction(&timeline, "procedure:gamma");
        }
        let watcher = ChronosWatcher::with_threshold(Arc::clone(&timeline), store, 3);
        let out = watcher.observe_once(100);
        assert_eq!(out.proposals.len(), 1);
        // Nothing to assert about a gate — the type has none by construction.
    }
}
