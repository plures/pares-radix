# ADR-0036: praxisbot Native Task Dashboard (Design)

- **Status:** Proposed (design-only — no implementation this pass)
- **Date:** 2026-07-23
- **Deciders:** praxisbot maintainers (kbristol)
- **Epic:** `praxisbot:px-native-task-dashboard` (P2)
- **Relates:** ADR-0027 (dev-lifecycle spine wiring), ADR-0034 (task-dispatch verb resolution)
- **Invariants:** C-PLURES-003 (all durable state in PluresDB, no ad-hoc state),
  C-DEV-001 (decisions in `.px`, side effects in Rust/exec), C-NOSTUB-001 (no hollow
  deliverables), C-DRIFT-001 (derived artifacts never hand-edited)

## Context

praxisbot currently tracks work across **four independent, unreconciled PluresDB
namespaces**, each with its own `.px` source of truth and none of them presented
together:

| Namespace                | Source `.px`                                   | Status vocabulary                                                                 | Purpose                                   |
|--------------------------|-------------------------------------------------|-------------------------------------------------------------------------------------|--------------------------------------------|
| `task:{id}`               | `praxis/procedures/task-system.px` + `task-management.px` | `pending, complete`                                                          | conscious agent commitments (task tools)   |
| `worktask:task:{task_id}` | `praxis/procedures/worktask.px`                | `planned, active, in_review, merging, done, abandoned`                              | git-worktree feature/bugfix/chore execution|
| `epic:registry:{id}`      | `development-guide/procedures/epic-registry.px`| `queued, in_progress, blocked, awaiting_approval, orphaned, done, abandoned`         | durable anti-orphan epic ledger            |
| `epic:gate:{epic_id}:{stage}` | `development-guide/procedures/epic-orchestration.px` | `open, passed, failed` (gate) / `analyze..land` (stage)                | per-stage evidence-gated governance        |

There is no single place to see "what is praxisbot working on right now, across all
four." A human (or the orchestrator) must query each namespace separately, and there is
no live, low-token surface — the existing `dashboard-stream.px` unifies ADO + resource
allocation + epic milestones, but does not aggregate task/worktask/gate detail, and is
scoped to the *human-in-Telegram* surface, not a general task view.

Per **C-PLURES-003**, any new dashboard state (render cache, cursor, last-rendered
counts) must live in PluresDB nodes — never a local file, in-memory cache, or ad-hoc
JSON ledger duplicating what `dashboard-stream.px` and `epic-registry.px` already prove
out.

This ADR is **design-only**: it fixes the entity/procedure shapes, the read boundary,
and the presentation contract so implementation can proceed as a single, reviewable
`.px`-first slice, without further architecture debate. **No `.px` files, Rust handlers,
or UI components are added in this pass.**

## Decision

### 1. The dashboard is a read-only aggregation view, never a second ledger

The dashboard **does not own task state**. It never writes to `task:*`,
`worktask:task:*`, `epic:registry:*`, or `epic:gate:*`. It only *reads* those four
namespaces (via PluresDB prefix scans, same primitive `epic-registry.px`'s heartbeat
sweep already uses) and writes exactly one small derived-cache namespace of its own:

```
dashboard:tasks:{surface_id}      — render cache (message id, counts, frozen flag,
                                     last_rendered_at) for ONE presentation surface
```

This mirrors the existing `dashboard:state:` entity in `dashboard-stream.px` — same
shape, scoped to tasks. No new persistence mechanism is invented (C-PLURES-003:
"durable nodes ARE the state," not a parallel file/cache).

### 2. Status vocabulary is translated at the read boundary, never duplicated in `.px`

Per **ADR-0034 §2** ("status vocabulary translation happens at the boundary, not
duplicated in `.px`"), the four namespaces' incompatible status enums are folded into
one **presentation-only** tri-state at the Rust/exec aggregation boundary — never as
parallel `when` branches copy-pasted across `.px` files:

| Unified `dashboard_status` | `task:*` (`pending/complete`) | `worktask:*` | `epic:registry:*` | `epic:gate:*` |
|---|---|---|---|---|
| `open`     | `pending`   | `planned, active`              | `queued, in_progress`            | `open`   |
| `waiting`  | —           | `in_review, merging`           | `blocked, awaiting_approval`     | —        |
| `done`     | `complete`  | `done`                          | `done`                            | `passed` |
| `stopped`  | —           | `abandoned`                     | `orphaned, abandoned`            | `failed` |

This table is the **single source of truth** for the mapping; the future
`dashboard_task_render` procedure and its Rust action handler both consult it — it is
never re-derived independently on either side (drift class this ADR explicitly closes
off, parity with ADR-0034's `Open`/`pending` mismatch postmortem).

### 3. One procedure file, following the proven zero-token tick pattern

A new `praxis/procedures/task-dashboard.px` (future implementation slice) will declare:

- **entity `task_dashboard_view`** (`prefix: "dashboard:tasks:"`) — `surface_id`,
  `message_id` (or `route_id` for a non-chat surface), `frozen`, `created_at`,
  `rendered_at`, `open_count`, `waiting_count`, `done_count`, `stopped_count`.
- **procedure `task_dashboard_tick`** — `trigger: periodic`, `side_effect: exec`,
  `actor: scripts/task-dashboard-tick.ps1`, identical shape to `dashboard_tick`
  (`dashboard-stream.px`): the actor performs the four prefix reads, applies the
  status-translation table, edits the surface in place. **Zero agent tokens per
  redraw** — same `dashboard_tick_is_zero_token` constraint class applies verbatim,
  scoped to this procedure name.
- **procedure `task_dashboard_get`** — `trigger: on_write`, a synchronous **read-only**
  command (`task:cmd:dashboard:get`) for on-demand "/tasks" style queries — same
  aggregation logic as the tick, but returns the payload directly instead of editing a
  live surface. No PluresDB write beyond the request/response envelope pattern already
  used by `list_feature`/`get_feature` in `worktask.px`.
- **constraint `task_dashboard_never_writes_source_namespaces`** — `severity: error`;
  the dashboard's own procedures may only `write_state` under `dashboard:tasks:*`, never
  under `task:*`, `worktask:*`, or `epic:*`. This is the structural guarantee that a
  "dashboard" cannot become a fifth, drifting ledger.
- **constraint `task_dashboard_tick_is_zero_token`** and
  **`task_dashboard_edits_in_place`** — copied in shape from `dashboard-stream.px`
  (`dashboard_tick_is_zero_token` / `dashboard_edits_in_place`), scoped to
  `task_dashboard_tick`.

### 4. Presentation surface: reuse the existing pinned-message pattern, don't invent a UI shell

For this pass, the **presentation surface is the same Telegram pinned-message pattern**
`dashboard-stream.px` already ships (`editMessageText`, edit-in-place, freeze-on-
milestone). A dedicated Praxis canvas/Svelte panel (per `praxis/ui/DESIGN-ui-schema-
engine.md`'s `DashboardGrid`) is an **explicit follow-on**, not required to ship value:
the chat surface is the one praxisbot users already look at, and it needs zero new
rendering infrastructure — only a new aggregation actor. Building a native canvas
surface before the aggregation/translation boundary is proven would risk the same
"per-UI authoring code" cost the UI-schema-engine ADR explicitly guards against
building prematurely (§8 non-goals: no new surface without a proven need).

**Recommended v1 shipping order (for the implementation epic, not this pass):**
1. `task-dashboard.px` entity + status-translation table + `task_dashboard_get`
   (on-demand, no scheduler yet) — smallest reviewable, testable slice.
2. `task_dashboard_tick` (periodic, zero-token) wired to a second pinned Telegram
   message (separate from `dashboard-stream.px`'s existing ADO/epic surface, to avoid
   overloading one message with unrelated content).
3. (Deferred, separate ADR if pursued) a canvas `/tasks` route using
   `DashboardGrid`/`Table` schema kinds already defined in `DESIGN-ui-schema-engine.md`.

## Consequences

- No new state store, cache file, or in-memory structure is introduced — praxisbot's
  task visibility becomes a **derived, PluresDB-backed view** over data that already
  exists, satisfying C-PLURES-003 by construction (the constraint this whole design
  answers to).
- The status-translation table is written once and consulted by both `.px` and its Rust
  action handler, closing off the exact drift class ADR-0034 documents as a real
  production regression (`Open` vs `pending` mismatch).
- A hard `never_writes_source_namespaces` constraint makes "the dashboard quietly
  becomes a second ledger" a build-time-checkable violation, not a matter of code
  review discipline.
- Deferring the canvas UI keeps the first implementation slice small and testable
  end-to-end (aggregate → translate → render one message) before any new rendering
  surface is justified.

## Non-goals (this ADR, and v1 if implemented)

- No new persistent task-tracking namespace. This is a **view**, not a fifth ledger.
- No canvas/Svelte UI component work in v1 — chat-surface only.
- No change to `task-system.px`, `worktask.px`, `epic-registry.px`, or
  `epic-orchestration.px` write paths. The dashboard is additive and read-only against
  them.
- No implementation in this pass — this ADR is design-only per the epic's DESIGN stage
  gate.

## Open decisions (need kbristol before implementation)

1. Separate pinned Telegram message for tasks, or a new section appended to the
   existing `dashboard-stream.px` unified message? *Leaning separate* (task detail is
   noisier / higher-churn than the epic/ADO summary; a single message risks becoming
   too dense — see `dashboard-stream.px`'s own rationale for why it replaced multiple
   ad-hoc pings with ONE surface, which argues for care before adding a second).
2. Tick interval for `task_dashboard_tick` — reuse 300000ms (5 min, `dashboard_tick`'s
   interval) or a tighter interval given tasks churn faster than epics? *Leaning
   reuse 300000ms for v1, revisit with real usage data.*
3. Does `task_dashboard_get` need pagination/filtering (by status, by repo) for v1, or
   is an unfiltered snapshot sufficient given current task volumes? *Leaning
   unfiltered v1, add filters only when a real volume problem appears (C-NOSTUB-001 —
   don't build filtering nobody has hit yet).*

## Verification (for the implementation pass, not this one)

- `node scripts/validate-px-grammar.cjs` against the new `task-dashboard.px` (dev-guide
  dialect subset per the `px-authoring` skill).
- Unit test the status-translation table exhaustively (all enum values from all four
  source namespaces map to exactly one `dashboard_status`).
- Constraint test: attempting a `write_state` under `task:*`/`worktask:*`/`epic:*` from
  `task-dashboard.px` procedures must fail `task_dashboard_never_writes_source_namespaces`.
- Loop proof: create a task in each of the four namespaces → tick fires → one message
  edited in place, all four represented with correct unified status.

## References

- `praxis/procedures/task-system.px`, `task-management.px`, `task-steering.px`,
  `task-evaluation.px`
- `praxis/procedures/worktask.px`
- `development-guide/procedures/epic-registry.px`, `epic-orchestration.px`
- `development-guide/procedures/dashboard-stream.px` (proven zero-token tick pattern)
- `praxis/ui/DESIGN-ui-schema-engine.md` (deferred canvas surface, `DashboardGrid` kind)
- ADR-0027 (dev-lifecycle spine wiring), ADR-0034 (task-dispatch verb resolution —
  status-vocabulary-at-the-boundary precedent)
