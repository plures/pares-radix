# ADR-0035: Resolving `@plures/design-dojo` Vendored-Shim Drift

- **Status:** Proposed (DESIGN stage only — no bulk copy/implementation authorized by this ADR)
- **Date:** 2026-07-23
- **Deciders:** kbristol (strategic directive), dev-lead orchestrator
- **Relates:** ADR-0024 §5 (design-dojo as canonical UI home), ADR-0032 (GraphView primitive),
  ADR-0033 (px composition language)
- **Invariants:** C-NOSTUB-001 (no stub/shim debt masquerading as done), C-TEST-001/002

---

## 1. Context

`pares-radix` depends on `@plures/design-dojo` two ways simultaneously:

1. **npm dependency** — `@plures/design-dojo-npm: npm:@plures/design-dojo@^0.17.0` (registry
   currently tops out at published `0.17.1`).
2. **Local vendored shim** — `packages/design-dojo/` (`workspace:*`), a **hand-maintained
   compatibility package** whose `package.json` description literally says:
   > "Local shim - re-exports npm @plures/design-dojo + adds missing components. Remove when npm
   > is updated to v0.13+"

The shim's `src/index.ts` re-exports most components from the npm package and then locally defines
16 components the npm package allegedly lacked: `SettingsPanel`, `Sidebar`, `Input`, `Select`,
`Dialog`, `DashboardGrid`, `FirstRunWizard`, `Heading`, `TextArea`, `Link`, `CodeBlock`, `Canvas2D`,
`PluginContentArea`, `DataGrid`, `SchemaForm`, `GraphView`.

Meanwhile, the **standalone `@plures/design-dojo` source repo** (separate repository; independently versioned; currently at package.json `1.55.34`, git HEAD `4fa2153`, 2026-07-21) is alive, organized (`src/lib/{primitives,layout,overlays,data,surfaces,feedback,app,...}`), and has
**361 source files** vs. the vendor shim's flat 24.

### Actual divergence found (not assumed)

| Vendored component | Exists in standalone repo? | Line-diff vs. standalone counterpart |
|---|---|---|
| SettingsPanel | ✅ `src/lib/app/SettingsPanel.svelte` | not diffed (bulk) |
| Sidebar | ✅ `src/lib/layout/Sidebar.svelte` | **285** differing lines |
| Input | ✅ `src/lib/primitives/Input.svelte` | **532** differing lines |
| Select | ✅ `src/lib/primitives/Select.svelte` | not diffed (bulk) |
| Dialog | ✅ `src/lib/overlays/Dialog.svelte` | **342** differing lines |
| DashboardGrid | ✅ `src/lib/layout/DashboardGrid.svelte` | **223** differing lines |
| FirstRunWizard | ✅ `src/lib/app/FirstRunWizard.svelte` | **656** differing lines |
| Heading | ✅ `src/lib/typography/Heading.svelte` | not diffed (bulk) |
| TextArea | ✅ `src/lib/primitives/TextArea.svelte` | not diffed (bulk) |
| Link | ✅ `src/lib/primitives/Link.svelte` | not diffed (bulk) |
| CodeBlock | ✅ `src/lib/primitives/CodeBlock.svelte` | not diffed (bulk) |
| Canvas2D | ✅ `src/lib/canvas/Canvas2D.svelte` | not diffed (bulk) |
| PluginContentArea | ✅ `src/lib/layout/PluginContentArea.svelte` | not diffed (bulk) |
| GraphView | ✅ `src/lib/app/GraphView.svelte` | **30** differing lines (closest match — recently ported, ADR-0032) |
| **DataGrid** | ❌ **not present upstream** | genuinely local-only (Phase B, schema-driven, built directly against `types-local.ts`) |
| **SchemaForm** | ❌ **not present upstream** | genuinely local-only (Phase B, same origin as DataGrid) |

Key findings:

- **13 of 16 "missing" components now exist upstream** — the shim's founding premise ("npm doesn't
  have these yet") is **stale**. npm registry is at `0.17.1`; the shim comment references
  "v0.13+" as the removal trigger, which has long since passed, yet the shim was never removed.
- **Every diffable component has drifted independently and substantially** (200–650+ line diffs),
  not just by formatting — these are now **effectively forked implementations**, not stale copies
  of the same code. `git log` on the vendored path shows repeated deliberate restoration commits
  (`20acb73 fix: restore Sidebar + SettingsPanel to local shim — npm API incompatible`,
  `0eb0734 fix: stable state — v0.17.0 npm + local Sidebar/SettingsPanel`) — i.e., the team already
  tried removing the shim components and reverted because the **npm-published API didn't match
  what pares-radix call sites expected**. This is real API drift, not just laziness.
- `GraphView` is the newest addition (ADR-0032, 2026-07-11) and has the smallest diff (30 lines) —
  evidence that when a component is added going *forward*, it can stay close to upstream if the
  upstream repo gets the change promptly; drift accumulates only when one side changes without the
  other.
- `DataGrid`/`SchemaForm` are legitimately pares-radix-native primitives (schema-driven grid/form,
  Phase B) with no upstream equivalent — these are **not drift**, they are **unpublished
  contributions** that should flow *into* design-dojo, not be reconciled against it.
- `types-local.ts` in the vendor package defines local-only types (`DataGridProps`, `SchemaField`,
  etc.) that duplicate/shadow what should be part of the design-dojo public type surface once
  DataGrid/SchemaForm are upstreamed.

### Additional risk found: dual resolution path (canvas-runtime)

`packages/canvas-runtime/package.json` depends on `@plures/design-dojo": "*"` **directly**,
bypassing the vendor shim entirely, while the main app depends on the shim
(`packages/design-dojo`, `workspace:*`). With both `@plures/design-dojo` (npm, hoisted) and
`@plures/design-dojo-npm` present in `node_modules`, **canvas-runtime and the main app shell are
not guaranteed to resolve the same implementation of overlapping components** (e.g. `GraphView`,
which is confirmed byte/line-diverged between the shim and standalone repo above). This is a
concrete correctness/consistency risk independent of the drift-cleanup plan below — it should be
resolved *first* (either point `canvas-runtime` at the shim too, or resolve everything to
upstream directly) so there is exactly one resolution path while the reconciliation work in §2
proceeds. No app screen currently imports the shim-only `DataGrid`/`SchemaForm`, so today's
blast radius for the dual-path risk is centered on the 14 overlapping components, `GraphView`
most acutely since its divergence is already confirmed.

### Cross-check against ADR-0019 (installer/packaging)

ADR-0019's multiplatform installer/packaging design is entirely Rust/Tauri/CI-side (`src-tauri`,
`release.yml`) and adds no new design-dojo consumers or requirements. The one relevant forward
link is ROADMAP Phase 4 ("Svelte GUI parity with Svelte TUI — design-dojo terminal theme"): no
terminal-theme components exist in either the shim or the standalone repo yet, so that item
remains a future design-dojo epic, not something this drift cleanup unblocks or blocks.

### Root cause
There is **no enforcement** preventing a plugin/consumer repo from silently forking UI primitives:
- No CI check diffing vendored files against the published/standalone package.
- No ownership boundary — `packages/design-dojo/src/*.svelte` are ordinary files anyone can edit
  in place during a pares-radix change, with no signal that they are a fork of another repo.
- No process for promoting local-only primitives (DataGrid, SchemaForm) upstream once proven.
- The shim's own removal criterion ("npm v0.13+") was met by `0.17.1` and nobody revisited it.

## 2. Decision

Adopt a three-part strategy: **stop the bleeding (ownership), reconcile (migration), then enforce
(prevent recurrence).** This ADR authorizes DESIGN only; no bulk copy/deletion happens until this
strategy is reviewed and a follow-up implementation ADR/PR is accepted.

### 2.1 Ownership & release strategy
- `@plures/design-dojo` (standalone repo) is the **single source of truth** for all shared UI
  primitives (per ADR-0024 §5). `packages/design-dojo/` in pares-radix is **not** a parallel
  product — it is a **compatibility shim only**, and must shrink to zero as fast as upstream sync
  allows.
- Any component that exists in both places must have **at most one authoritative source**: the
  standalone repo. Vendored copies are transitional bridges, never permanent forks.
- `DataGrid` and `SchemaForm` are reclassified as **upstream contributions owed**, not vendor drift — they get a dedicated small PR into the standalone design-dojo repository (own review, own semver bump), not a copy-paste reconciliation.
- Release cadence: design-dojo standalone repo cuts a release for every component it gains from pares-radix contributions or fixes drift-discovered bugs; pares-radix bumps its `@plures/design-dojo-npm` pin promptly after (target: within one sprint) rather than papering over the gap with a local shim edit.

### 2.2 Migration approach (for follow-up implementation PR — not this ADR)
1. **Triage per component** (not bulk): for each of the 13 drifted components, determine whether
   the *pares-radix* variant or the *standalone* variant is closer to "correct" per current
   requirements — likely mixed, since some diffs are pares-radix bugfixes never upstreamed and
   some are standalone improvements never pulled down.
2. **Upstream first** for genuinely-local primitives: PR `DataGrid` + `SchemaForm` (with
   `types-local.ts` types folded into the public type surface) into the standalone repo before
   touching anything else.
3. **Reconcile drifted components one at a time**, smallest-diff first (`GraphView` at 30 lines is
   the trivial pilot case to prove the reconciliation workflow), each as its own small PR with its
   own diff review and test pass — never a single mass "sync everything" commit.
4. **Delete from the vendor shim only after** the reconciled/upstreamed version is published to npm
   and pares-radix's pin is bumped and verified (`svelte-check`, existing route/story smoke tests)
   against the npm version, matching the exact pattern already tried in `20acb73`/`0eb0734` but
   this time backed by enforcement (below) so it doesn't silently re-drift.
5. Target end state: `packages/design-dojo/` shrinks to nothing (or a thin re-export file kept only
   if a genuine local override is still required, clearly labeled and covered by the CI check in
   §2.3), and `pares-radix` package.json depends on `@plures/design-dojo` directly (no shim
   indirection layer).

### 2.3 Enforcement to prevent renewed drift
- **CI drift check**: a scheduled/PR-triggered job that, for every file in `packages/design-dojo/`
  whose basename matches a file in the standalone repo (path resolved via a manifest checked into
  `packages/design-dojo/UPSTREAM_MAP.json` mapping vendor filename → standalone repo path), fails
  the build if the vendored copy differs from the pinned npm-published version's corresponding
  source and the diff isn't accompanied by a `DRIFT.md` entry explaining why (temporary override,
  upstream PR link, expected removal date).
- **No silent local edits**: any change to a file under `packages/design-dojo/src/*.svelte` that
  has an upstream counterpart must be accompanied by either (a) a linked upstream PR/issue in
  `design-dojo`, or (b) removal of the local override once upstream ships. Enforced via PR
  template checklist + the CI check above (fails without a `DRIFT.md` entry).
- **Shim removal criterion becomes a tracked, dated obligation**, not a comment nobody revisits:
  each entry in `UPSTREAM_MAP.json` carries a `removeAfterNpmVersion` field; CI flags (does not
  necessarily hard-fail, but surfaces loudly) any entry where the current npm pin already satisfies
  `removeAfterNpmVersion` — this alone would have caught today's stale shim (target `0.13+` vs.
  actual pin `0.17.x`).
- **Ban new local-only primitives in the vendor package** going forward: any *new* shared UI need
  gets built directly in the standalone `design-dojo` repo (consumed via `workspace:*`/local link
  during co-development if needed) rather than added to `packages/design-dojo/src/*.svelte` as a
  "temporary" addition — this is exactly how `DataGrid`/`SchemaForm` ended up stranded.

## 3. Consequences

**Positive** — single source of truth restored; the 200–650-line-diffed components stop silently
diverging further; `DataGrid`/`SchemaForm` become available to every design-dojo consumer, not just
pares-radix; CI enforcement makes "shim never gets removed" structurally harder to repeat; smallest
diff (`GraphView`) provides a low-risk pilot for the reconciliation workflow before tackling the
large diffs (`FirstRunWizard` at 656 lines will need the most care/dedicated review).

**Costs/risks** — reconciling 13 drifted components is real work spread over multiple PRs/sprints,
not a single sweep; some pares-radix-side fixes embedded in the drifted diffs may be lost or need
re-discovery if not carefully triaged before reconciliation; standalone repo maintainers must accept
and prioritize the `DataGrid`/`SchemaForm` + fix-upstreaming PRs promptly or pares-radix will be
tempted to re-fork under deadline pressure — this ADR's enforcement mechanism only works if upstream
turnaround is fast enough to not be a bigger friction than the shim was.

## 4. Explicitly out of scope for this ADR
- No files are copied, deleted, or modified in either repo by this ADR.
- No `UPSTREAM_MAP.json`, CI job, or PR template is created yet — those are implementation-stage
  deliverables of the accepted follow-up PR.
- No component-by-component reconciliation begins until this design is reviewed and accepted.

## 5. Next steps (pending review/acceptance of this ADR)
1. Review/accept this ADR.
2. Open implementation PR #1: `UPSTREAM_MAP.json` + CI drift-check job (enforcement scaffolding,
   no component changes).
3. Open upstream PR: `DataGrid` + `SchemaForm` → standalone `design-dojo` repo.
4. Open reconciliation PR: `GraphView` (pilot, smallest diff) — prove the workflow.
5. Sequence remaining 12 components by diff size / risk, each its own PR.
6. Retire `packages/design-dojo/` shim once all entries are reconciled and the npm pin covers them.

## 6. References
ADR-0024 §5 (design-dojo canonical UI home, subpath consumption pattern), ADR-0032 (GraphView,
most recent addition — smallest drift, proof that fast upstream sync prevents divergence);
`packages/design-dojo/package.json` (shim description, stale removal criterion); standalone design-dojo repo git HEAD `4fa2153` (2026-07-21); pares-radix git history `20acb73`/`0eb0734` (prior attempted-then-reverted shim removal, evidence of real API drift, not just process laziness); C-NOSTUB-001.
