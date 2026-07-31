# ADR-0018: Procedure-Native Plugin Integration (Epic A Realization)

## Status
**PROPOSED (design-only)** — 2026-07-23

## Decision Makers
kbristol (Paradox)

## Context

`design/EPIC-COHESION-PARITY.md` (Epic A — Cohesion Parity, directive kbristol #40346,
2026-07-06) reframed what "plugin integration" means for pares-radix/agens relative to
OpenClaw. The ground-truth recon (2026-07-06) established two load-bearing facts that
this ADR takes as given and does not re-litigate:

1. **Dependency direction is inverted from the naive assumption.** `pares-radix` is the
   platform runtime (Tauri app + 11 Rust crates, praxis engine, plugin loader, PluresDB
   bridge). `pares-agens` links radix *as a library* and adds the agent/cognition layer,
   channel adapters, MCP server, and OpenClaw-migration tooling. Radix does not depend on
   agens (`git grep pares-agens` in radix → 0 hits).
2. **The OpenClaw-parity target is the `pares-agens` host**, not the `pares-radix-app`
   Tauri desktop shape. These are two separate deployment shapes; Epic A/B scope only the
   `pares-agens` host.

Separately, `docs/ROADMAP.md` and `docs/decisions/ADR-0010-agens-first-plugin.md` /
`ADR-0011-plugin-security.md` describe an **older, still-partially-live plugin-platform
roadmap**: "Phase 3 — Plugin Migration (Q3 2026)" porting FinancialAdvisor/vault/sprint-
log/netops as `RadixPlugin`s into a UI-level plugin framework (`pares-modulus`, manifest +
loader + marketplace, `docs/architecture/plugin-system.md`, `docs/PLUGIN-AUTHOR-GUIDE.md`).
That roadmap is **product-plugin infrastructure** (domain apps as installable UI plugins
with routes/nav/settings/dashboard widgets) — a different concern from **OpenClaw-parity
plugin *replacement*** (MCP tools, skill_workshop, channels, cron, approvals) that Epic A
is actually about.

This ADR's job is narrow: **turn Epic A's reframed parity matrix into an actionable,
procedure-native integration plan**, distinguishing genuine remaining gaps from backlog
items that predate the recon and no longer describe the real integration surface.

## Verification performed for this ADR (2026-07-23)

- Re-read `design/EPIC-COHESION-PARITY.md` in full (all phases, matrix, DoD).
- Confirmed no open GitHub issues/PRs in `plures/pares-radix` reference "plugin backlog"
  or "plugin-integration" as a tracked epic (`gh issue list --search` returned empty).
  The only open PR touching related surface is #473 (praxisbot task-obligations clippy
  fix — unrelated to plugin integration).
- Confirmed the praxis engine + plugin-loader mechanism cited in the matrix is real:
  `docs/architecture/plugin-system.md` documents the live `RadixPlugin` contract
  (manifest, UI integration, praxis `expectations`/`rules`/`constraints`, lifecycle hooks,
  `PluginContext`). This is the **existing native mechanism** — it is a UI/product-plugin
  system, distinct from the MCP-tool/channel/cron surface the parity matrix tracks.
  `ADR-0010` frames plugins as praxis-declared (Facts/Events/Rules/Constraints, no
  imperative loaders) — consistent with the "procedure-native" framing this ADR extends.
  `ADR-0017` (channel-agnostic agent loop, ACCEPTED 2026-05-16) already establishes the
  principle this ADR needs for the channel/MCP gap: the agent loop, not the channel, is
  the capability boundary — reinforces that Epic A's remaining channel work is adapter
  breadth, not agent-loop rearchitecture.
- Cross-checked Epic A's phase log: A0 (baseline) is marked done with zero UNKNOWNs; four
  of five confirmed gaps in A1 are marked ✅ DONE (`skill_workshop`, `apply_patch`,
  `failureAlert`, `praxis-evaluate` faithfulness fix — the last needs a live gateway
  reload, deferred to user per config policy). This means **the actionable remaining
  scope is much smaller than the original epic framing implied** — most of what looked
  like a large "plugin integration" effort is already closed.

## Decision

Adopt the parity matrix in `EPIC-COHESION-PARITY.md` as the authoritative gap list, and
explicitly **retire the Q3-2026 ROADMAP "Plugin Migration" phase as unrelated scope** for
Epic A/B purposes (it may proceed independently as UI/product-plugin work, but must not be
conflated with OpenClaw-parity closure). Going forward, plugin-integration work under this
epic is procedure-native: every remaining gap is closed by (a) a `.px` law/constraint
where the gap is a *policy* gap, or (b) a narrowly-scoped Rust/crate change gated by the
existing `dev-lifecycle` staged-subagent flow where the gap is a *mechanism* gap — never
by a new bespoke plugin-loader abstraction.

### Ruled out (obsolete backlog assumptions)

| Backlog item | Why it's obsolete for Epic A/B |
|---|---|
| "Port FinancialAdvisor/vault/sprint-log/netops as `RadixPlugin`s" (ROADMAP Phase 3) | UI-level product plugins, orthogonal to OpenClaw cohesion parity. No cohesion feature in the matrix depends on this. Leave to its own roadmap track. |
| "Wire Tauri → connect PluresDB → build plugin loader → mount components" as the mental model for plugin work (ADR-0010's "old thinking (wrong)") | Already explicitly rejected in ADR-0010; the praxis-first model (Facts→Rules→Constraints, everything else derived) is the standing decision. Nothing in Epic A reopens this. |
| Assuming `pares-agens` is "a radix plugin" in the OpenClaw-parity sense | ADR-0010's title calls agens "the first radix plugin" architecturally (praxis-declared), but Epic A's ground truth is that agens is the *host* that links radix as a library for the OpenClaw-replacement deployment shape. These are compatible statements about different layers (praxis-plugin-contract vs. binary/deployment topology) but must not be read as "agens is a plugin to be migrated" — it is the target runtime. |
| Discord/Signal channel adapters, Hyperswarm P2P as *required* for Telegram-first parity | Epic A explicves defers both unless required; SSH remote-exec already covers the node case radix needs. Do not backlog these as blocking gaps. |
| "Plugin marketplace install ≠ authoring" as an open gap | Closed — `skill_workshop` author/propose/apply/reject/quarantine loop shipped (agens `b803ef5`, 28 tests). Marketplace signed-install and skill-authoring are now both native and distinct, by design (consume-remote vs. author-local). |

### Genuine remaining gaps (procedure-native plan)

Ordered by what Epic A's own A1 phase left unclosed, each phrased as an actionable
next step — design-only here; implementation goes through `pares-radix-dev-lifecycle`:

1. **Approval-gate + sandbox + elevated enforcement.** `tool_governance.rs` has real
   types (`ToolPolicy{approval_required,sandboxed,blocked_patterns}`,
   `GovernanceVerdict`) and hard-blocks dangerous patterns, but `approval_required`
   currently only warns-and-proceeds (`AllowWithApprovalWarning`, explicitly documented
   as "Phase 5+"), `sandboxed` is an unenforced field, and `elevated` mode doesn't exist.
   **This is the epic's largest remaining gap and the one with real security stakes.**
   Plan: express the approval/sandbox/elevated semantics as `.px` constraints first
   (gate definitions: what must be true before a governed tool call proceeds), then wire
   enforcement in `tool_governance.rs` against those constraints — mirroring how
   `blocked_patterns` is already both a policy fact and an enforced hard-block. Elevated
   mode needs an explicit new `GovernanceVerdict` variant plus a policy source (likely a
   dedicated `.px` gate keyed off caller trust level) before any enforcement code lands.
2. **Praxis-evaluate live activation.** The `mcp-dev-server` faithfulness fix (scope
   spread + `.includes()`/arithmetic support) is merged (radix `5505009`) but not yet
   live — needs a Radix MCP gateway reload. This is an operational step, not a design
   gap; track it as a deploy/verify follow-up, not new scope.
3. **A2 — prove ≥ parity via channel-independent tests.** No test evidence yet that the
   18 `.px` laws load and enforce inside radix's engine, or that each NATIVE/SUPERIOR
   matrix cell is backed by an MCP/HTTP-level test (per C-TEST-002, never single-adapter).
   This is the actionable next epic phase and should be the next `dev-lifecycle` task:
   one test per matrix row, executed against MCP/HTTP, not Telegram.
4. **A3 — document the SUPERIOR deltas** (RSI rails, in-process `.px` enforcement,
   PluresDB reactive memory) as the explicit "why migrate" narrative feeding Epic B. Not
   a code gap — a documentation deliverable, cheap to close once A2 evidence exists.

### Non-goals (explicitly out of scope for this ADR and for Epic A/B)

- Rebuilding or extending the UI-level `RadixPlugin`/`pares-modulus` product-plugin
  system. That track is independent and unaffected by this decision.
- Discord/Signal/other channel adapters, Hyperswarm P2P — deferred, not gap-closing work,
  unless a concrete requirement emerges.
- Any new plugin-loader or plugin-abstraction mechanism. The mechanism already exists
  (praxis engine + `.px` loader for logic; `tool_governance`/`marketplace`/`agenda` crates
  for platform surface); the work is enforcement and verification, not construction.

## Consequences

- Epic A's remaining actionable surface shrinks to three items: approval/sandbox/elevated
  enforcement (security-bearing, real code change), praxis-evaluate gateway reload
  (ops step), and A2/A3 (test + documentation phases). This is a materially smaller scope
  than "plugin integration" implies, and should be communicated as such to avoid
  re-litigating already-closed gaps (`skill_workshop`, `apply_patch`, `failureAlert`).
- Any future work item that says "port X as a plugin" for OpenClaw-parity reasons should
  be rejected at triage — redirect to the ROADMAP Phase 3 product-plugin track if it's
  genuinely about domain-app UI plugins, or to this ADR's gap list if it's about
  OpenClaw-cohesion parity. The two must not be merged in planning.
- The approval/sandbox/elevated gap (#1 above) is the one item here with real security
  exposure (warn-and-proceed today) and should be prioritized over A2/A3 documentation
  work if resourcing forces a choice.

## Next Steps (design-only; no code in this ADR)

1. Land this ADR + open the epic-tracking PR (this change).
2. File the approval/sandbox/elevated design as its own `.px`-first task through
   `pares-radix-dev-lifecycle` (analyze → fix → test → deploy → verify), scoped strictly
   to items in the "Genuine remaining gaps" list above.
3. Schedule A2 (channel-independent parity tests) as the next dev-lifecycle task once the
   governance work is scoped, so test coverage lands against the enforcement change
   rather than the current warn-and-proceed behavior.
4. Update `EPIC-COHESION-PARITY.md` to link this ADR and mark the ROADMAP Phase 3 overlap
   as explicitly out-of-scope, so future readers don't re-conflate the two tracks.

## References

- `development-guide/design/EPIC-COHESION-PARITY.md` (Epic A source of truth)
- `memory/epic-a-a0-parity-2026-07-06.md`, `memory/recon-radix-agens-2026-07-06.md`
- `docs/decisions/ADR-0010-agens-first-plugin.md`, `docs/decisions/ADR-0011-plugin-security.md`
- `docs/adr/ADR-0017-channel-agnostic-agent.md`
- `docs/architecture/plugin-system.md`, `docs/ROADMAP.md`
- `tool_governance.rs` (L8-9: "approval UI is Phase 5+", verified verbatim per Epic A A0)
