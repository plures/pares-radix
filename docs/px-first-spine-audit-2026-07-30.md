# .px-First Spine Audit — pares-radix + pares-agens (2026-07-30)

**Epic:** `plures:px-first-spine-refactor-radix-agens`
**Context:** `praxisbot:px-native-task-dashboard` has been carrying a phantom blocker
("needs .px-first spine refactor of pares-radix and pares-agens") with no
tracked epic behind it. `plures:px-first-architecture-refactor` (PR
pares-agens#681, merged) only covered PluresDB's reactive-procedure layer —
it did **not** sweep pares-radix's spine crate or pares-agens' orchestrator
crate. This document is that sweep: a grep-based inventory (no rewrite
attempted this pass) plus a staged plan to close the gap.

## 1. The actual disconnect blocking px-native-task-dashboard

Two runtimes exist in pares-radix and neither talks to the other:

1. **`radix-core::spine::runtime::build_reactive_runtime_with_subagent`**
   (`crates/radix-core/src/spine/runtime.rs`) — the real, full-featured
   assembly path. It builds a `CompositeActionHandler`
   (`crates/radix-core/src/spine/actions.rs`) wired with:
   - `ToolDispatchActionHandler`
   - task grounding (`TaskManager`)
   - `TaskDispatchActionHandler` (autonomous task dispatch)
   - `TaskHandoffActionHandler` (custody-transfer actions)
   - `SubagentActor` (task-completion seam)
   - **`TaskDashboardActionHandler`** (`crates/radix-core/src/spine/task_dashboard_actions.rs`,
     528 lines) — this is where PR#559's native task dashboard aggregation
     lives, gated behind the spine's `.px` procedure dispatch.

2. **`pares-radix-svc`** (`crates/pares-radix-svc/src/lib.rs`, PR#571, ADR-0018
   DEV stage) — the new always-on service binary. It does **not** call
   `build_reactive_runtime_with_subagent` or construct a
   `CompositeActionHandler` at all. It imports a *different, simpler* type:
   `pluresdb_procedures::agens::AgensRuntime`, holds one long-lived instance,
   and drives it via a tokio interval calling `process_due_timers` /
   `poll_events`. There is no `TaskDashboardActionHandler`, no
   `ReactiveRegistry`, no spine pipeline anywhere in this crate.

**Conclusion:** PR#571 intentionally scoped pares-radix-svc's DEV stage to a
minimal `AgensRuntime` loop (per its own PR notes: "Known gaps ... out of
scope for this DEV-stage PR per the ADR's own staged rollout"). It was never
wired to the spine, so `TaskDashboardActionHandler` — and every other
`CompositeActionHandler`-gated action — is simply **absent** from the
service process that praxisbot actually runs. This is not a "needs .px-first
refactor" problem; it is a **missing assembly wiring problem**: the DEV-stage
service never grew past its minimal loop into the full spine path described
in ADR-0018's own diagram comments (`runtime.rs` lines 18-22).

The "architecture decision" the dashboard is blocked on is therefore real,
but mischaracterized. The actual open question is:

> Does `pares-radix-svc` assemble the full spine (`CompositeActionHandler` +
> `TaskDashboardActionHandler` + friends) going forward, or does the task
> dashboard get re-implemented as a thin read path directly against
> `AgensRuntime`'s state store, independent of the spine?

Both are legitimate answers. Neither requires "put decision logic in .px
first" as a *precondition* — that conflation is what made the blocker
phantom. The .px-first sweep below is real, valuable, independently-tracked
work; it does not itself resolve the full-spine-vs-minimal-route question,
but Stage 1 below (a design decision) does.

## 2. Rust decision/routing/selection/scoring inventory

Grep-based pass across both repos for scoring functions, dispatch/match
tables doing business logic, and routing decisions (excludes tests/target).

### pares-radix

| File | Lines | Finding |
|---|---|---|
| `crates/radix-core/src/model_pool/selection.rs` | 311 | `score_model()` — hand-rolled weighted scoring (capability score, RSI/performance score, cost score, speed score) blended by `SelectionWeights`. Pure decision logic, no IO. **Prime .px-extraction candidate** — this is exactly the shape of business logic the `lifecycle_no_rust_business_logic` constraint targets. |
| `crates/radix-core/src/spine/model_selection_actions.rs` | 401 | Action-handler wrapper around model selection; calls into `selection.rs`. Should become the IO boundary that invokes a `.px` `select_model` procedure instead of the current Rust scoring call. |
| `crates/radix-core/src/threading/router.rs` | 389 | Message/thread routing decisions (which thread/session a message routes to). Contains `match`-based routing tables — needs closer read to separate pure IO (thread lookup) from actual routing decisions before scoping a .px extraction. |
| `crates/radix-core/src/spine/task_dashboard_actions.rs` | 528 | Aggregation + status-derivation logic for the task dashboard (PR#559). Contains business rules for what counts as "active"/"blocked"/"stale" — candidate for a `.px` `derive_task_status` procedure, separate from the assembly-wiring gap in \S1. |
| `crates/radix-core/src/spine/task_dispatch_actions.rs`, `task_handoff_actions.rs` | — | Dispatch/handoff action handlers; already IO-boundary shaped (call into `TaskDispatcher`/`ConditionalTaskStore`), lower priority — these look like legitimate IO glue, not hidden business logic. Needs a closer pass in Stage 2 to confirm. |
| `crates/radix-core/src/lifecycle/actions.rs`, `executor.rs`, `task_executor.rs`, `plugins/manifest.rs` | — | Flagged by grep for `match`/dispatch tables; not yet triaged as decision-logic vs. plumbing. Included in Stage 2 scope. |

### pares-agens

| File | Lines | Finding |
|---|---|---|
| `crates/core/src/orchestrator/px_bridge.rs` | 286 | Already the correct pattern — holds loaded `.px` procedures keyed by name, orchestrator logic calls out to them. This is the model to replicate elsewhere, not a violation. |
| `crates/core/src/orchestrator/actions.rs` | 1412 | Largest file in the sweep. Contains `match event_type`, `match role`, `match tier` (temperature-by-tier selection), `match name` (dispatch table), and pattern-matching helpers (`match_against_patterns`). Mix of legitimate IO (streaming completion calls) and embedded decision logic (tier→temperature mapping, event-type routing). **Needs a line-level triage pass (Stage 2)** — too large to blanket-classify from grep alone. |
| `crates/core/src/headroom.rs` | 2114 | Largest file in either repo. Grep-flagged for dispatch/match; likely token-budget/headroom *policy* decisions (what to trim, what to keep) — high-value .px-extraction candidate given its size, but requires a dedicated read-through, not a grep guess. |
| `crates/core/src/agent.rs`, `orchestrator/context_manager.rs` | — | Flagged, not yet triaged. |
| `crates/mcp-server/src/radix_handler.rs`, `agens-plugin/src/agent_commands/runtime.rs`, `channels/src/active_turns.rs`, `cli/src/main.rs` | — | Flagged, lower priority (CLI/MCP glue, likely thinner than core crates). |

**What this inventory is NOT:** a rewrite, a line-count promise, or a
finished classification of every flagged file. `actions.rs` and
`headroom.rs` in particular are large enough that grep alone
over/under-classifies; Stage 2 exists specifically to do the real read.

## 3. Staged dev-lifecycle plan

No big-bang rewrite. Each stage gates the next; stages are scoped to be
independently shippable.

### Stage 0 — Design (this artifact + one decision)
- [x] Grep-based inventory (this document).
- [ ] **Decision needed from kbristol or default-to-spine-per-ADR-0018:**
      resolve \S1's full-spine-vs-minimal-route question for
      `pares-radix-svc`. Recommendation: extend `pares-radix-svc` to call
      `build_reactive_runtime_with_subagent` (the existing, tested assembly
      function) instead of constructing a bare `AgensRuntime`, so
      `TaskDashboardActionHandler` and the rest of the composite handler
      become live in the service process. This reuses existing, tested code
      — it is wiring, not new logic — and is the natural next PR after #571
      per ADR-0018's own staged rollout (DEV stage explicitly deferred this).
- [ ] File as a tracked ADR update or new ADR under
      `praxis/decisions/` in pares-radix if the decision changes ADR-0018's
      scope.

### Stage 1 — Dev (assembly wiring, unblocks the dashboard)
- [ ] Wire `pares-radix-svc`'s `ServiceLifecycle` to construct the full
      spine (`build_reactive_runtime_with_subagent`) instead of a bare
      `AgensRuntime`, reusing the existing `StateStore`/`ConversationStore`
      already opened in `lib.rs`.
  - Real dependencies to resolve: tool dispatcher wiring, `praxis_dir`
    resolution for the service context, subagent spawner (may be `None`
    initially — service processes may not spawn subagents).
  - Scope check: this alone is enough to make `TaskDashboardActionHandler`
    reachable from the service. It is the direct unblock for
    `px-native-task-dashboard`, independent of any .px extraction below.
- [ ] `cargo test -p pares-radix-svc` + new smoke test asserting the
      dashboard action handler responds over the service's HTTP surface.

### Stage 2 — .px extraction (the actual px-first sweep, incremental)
Ordered by value/size, one PR per row, each independently tested:
1. `model_pool::selection::score_model` → `.px` `select_model` procedure
   (highest-confidence pure business logic, smallest blast radius).
2. `task_dashboard_actions.rs` status-derivation rules → `.px`
   `derive_task_status` procedure.
3. `pares-agens::orchestrator::actions.rs` tier→temperature mapping and
   event-type routing table → extracted after a dedicated line-level triage
   (file too large for a single PR; split into at least 2).
4. `pares-agens::core::headroom.rs` — dedicated read-through first (no
   extraction decision until then); likely 2+ PRs given size.
5. Remaining flagged-but-untriaged files (`threading::router.rs`,
   `lifecycle::actions.rs`, `context_manager.rs`, etc.) — triage each before
   committing to extraction; some may turn out to be legitimate IO glue.

### Stage 3 — Document
- [ ] Update `development-guide` repo-routing / DEVELOPMENT-LIFECYCLE notes
      if the spine assembly pattern changes (Stage 1 result).
- [ ] Record each Stage 2 extraction as its own ADR entry in the relevant
      repo's `praxis/decisions/`.

### Stage 4 — QA
- [ ] Per PR: `cargo test --workspace`, `cargo clippy --workspace -- -D
      warnings`, build-the-binary-run-the-binary smoke test hitting the real
      HTTP surface (per repo's test-first gate).
- [ ] End-to-end: with Stage 1 landed, verify `TaskDashboardActionHandler`
      responds through the actual `pares-radix-svc` binary, not just the
      spine's existing unit tests.

### Stage 5 — Deploy
- [ ] Stage 1 ships to whatever target runs `pares-radix-svc` (this repo's
      own deploy path — not gated on praxisbot specifically, per the
      documented praxisbot-is-a-deploy-target-not-a-gate correction).

### Stage 6 — Verify
- [ ] Confirm on the live service that `px-native-task-dashboard` actually
      renders dashboard data end-to-end (close the loop — don't declare
      victory on "tests pass").
- [ ] Close out the phantom-blocker note on `praxisbot:px-native-task-dashboard`
      once Stage 1 + Stage 6 are done, referencing this document and the
      landing PR.

## 4. Does this unblock px-native-task-dashboard?

**Not yet, but it identifies the exact, small fix that will.** The blocker
was framed as "needs a .px-first spine refactor" — a large, vague,
open-ended precondition. The real gap is narrow: **Stage 1 is a wiring fix**
(construct the existing, tested `build_reactive_runtime_with_subagent`
inside `pares-radix-svc` instead of a bare `AgensRuntime`). That is
buildable and testable independently of the broader Stage 2 .px-extraction
sweep. Recommendation: land Stage 1 first as its own PR to unblock the
dashboard immediately; treat Stage 2 as ongoing, separately-tracked
px-first debt (per the epic), not a precondition for Stage 1.
