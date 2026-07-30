# ADR-0038: Radix policy + settings engine (design stage)

- Status: Proposed (design stage only — no implementation code)
- Date: 2026-07-29

## Context

kbristol decided (2026-07-29) that policies and settings across Radix and its extensions/plugins should be governed by a **general-purpose, fully customizable policy+settings engine built from `.px` (Praxis) procedures**, shipping with reasonable secure-by-default behavior. This is `program:radix-policy-settings-engine`.

Explicitly out of scope for now: **per-host custom configs for praxisbot/surface/kbristol-devbox are DEFERRED** until the engine itself ships. Directional default for how per-instance customization categories should behave: **match how OpenClaw behaves out of the box** (OpenClaw's own settings/policy resolution model is the reference UX, not a novel scheme).

### What already exists (grounded in current `.px` capability, not invented)

Reviewed the live `praxis/` tree in `pares-radix` (this worktree, `origin/main` @ `e900a56`):

- **`praxis/directives.px`** — top-level org-policy procedures (`route_code`, `prioritize_work`, `validate_architecture`, `guide_lifecycle`, `detect_anti_patterns`). Pattern: `db_get {key: "namespace:category"} -> $var`, evaluate, `emit {type: "...", ...}`, `return`. Keys are colon-namespaced strings (`plures:repos:registry`, `plures:routing:rules`, `plures:lifecycle:seven-pillars`) resolved via a generic key/value store (PluresDB), not a bespoke schema per procedure.
- **`praxis/procedures/constraint-eval.px`** — the existing **constraint model**: intended `constraint:<name>` keys hold a definition (`given`, `check`, `severity: error|warning|log`), enumerated via `db_get_prefix {prefix: "constraint:"}`. Note: the current implementation’s `db_get`/`pluresdb_write` calls use the literal key `"constraint:"` (e.g. `constraint-eval.px:57,76,81`), so aligning reads/writes with per-name keys is a prerequisite for treating this as a general policy primitive.
- **`praxis/spine/*.px`** (`spine.px`, `routing.px`, `conversation.px`, `heartbeat.px`, `tool_execute.px`, `model_invoke.px`) — the dataflow "spine" that procedures plug into via named queues (`fact_name` in/out). This is the wiring substrate a policy engine would ride on, not something new to build.
- Procedure conventions observed: `procedure <name>(<params>) -> <result> [into "<queue>"]:`, `given: "<human-readable rationale>"`, `db_get`/`db_get_prefix`/`pluresdb_write` for persistence, `when <cond>: ... end` for conditionals, `loop over $x as y: ... end` for iteration, `emit {type: ..., ...}` for observability events, `return <value>`.
- **No existing generalized "settings" or "policy scope/override precedence" concept** — `constraint-eval.px` is flat (one global namespace of constraints, no scope/host/plugin dimension). This ADR's contribution is generalizing that flat model into a scoped one, using the same primitives (namespaced keys + prefix queries + severity/enforcement), not inventing a new storage or execution mechanism.
- MCP surface (`radix__praxis-*` tools, per `TOOLS.md`/`packages/mcp-dev-server`): `listRules`, `addConstraint`, `evaluate` (phase-based, e.g. `phase: "code_review" | "pre_push" | "retro"`) are the operator-facing entry points already wired to this same `constraint:` keyspace. The design below is expressed so it can be exposed through the same tool family (`radix__policy-get`, `radix__policy-set`, `radix__policy-evaluate`) without new plumbing.

## Decision

Build the policy+settings engine as an extension of the existing constraint/key-value model, expressed entirely as `.px` procedures — no new host language, no new storage engine, no bespoke config format.

### 1. Policy entity model

A **policy** (this ADR uses "policy" for behavior-governing rules and "setting" for plain configuration values — both share one storage/precedence mechanism, differing only in whether they carry an enforceable `check`):

```
policy:<scope-path>:<name>
  given:        string   # human-readable rationale (required, matches existing convention)
  kind:         enum(constraint, setting)
  value:        any      # for kind=setting: the configured value
  check:        string?  # for kind=constraint: boolean expression, evaluated like constraint-eval.px today
  severity:     enum(error, warning, log)   # for kind=constraint only
  secure_default: bool   # true if this ships as part of the secure-by-default baseline (see §4)
  overridable:  bool     # false = hard floor, cannot be relaxed by any lower scope
  updated_by:   string   # actor/session that last wrote this entry
  updated_at:   timestamp
```

This is a direct generalization of the existing `constraint:<name>` record in `constraint-eval.px` — same fields (`given`, `check`, `severity`) plus scope-path key structure, a `kind` discriminator (so plain settings reuse the same store instead of needing a second table), and the two new override-control fields (`secure_default`, `overridable`) needed for §3/§4.

### 2. Setting scopes

Scope is encoded directly in the key path, following the existing `namespace:category` convention (e.g. `plures:repos:registry`) extended one level:

```
policy:global:<name>                          # org/product-wide default
policy:plugin:<plugin-id>:<name>               # per-extension/plugin override
policy:host:<host-id>:<name>                   # per-host override (praxisbot/surface/kbristol-devbox)
```

- `global` is the only scope required to exist at engine-ship time. `plugin` and `host` scopes are supported by the schema from day one (so extensions/plugins can register their own settings immediately), but **populating `host:*` entries for specific real hosts (praxisbot/surface/kbristol-devbox) is explicitly deferred** per the 2026-07-29 decision — this ADR defines the mechanism, not the per-host content.
- Lookup is a **prefix-aware, most-specific-wins read**: given a request for `<name>` under plugin `P` on host `H`, resolve in order `policy:host:H:<name>` → `policy:plugin:P:<name>` → `policy:global:<name>`, first hit wins. This mirrors `db_get_prefix {prefix: "constraint:"}` already used for bulk enumeration — no new query primitive needed, just a resolution procedure that tries three `db_get` keys in specificity order.

### 3. Override precedence rules

1. **Most-specific scope wins**, per §2 (host > plugin > global).
2. **`overridable: false` is a hard floor.** If the `global` (or any less-specific) entry for a `<name>` has `overridable: false`, no `plugin` or `host` entry may relax it — a write attempt at a lower scope for a non-overridable policy is rejected at write-time (`update_policy` returns `{status: "rejected", reason: "not_overridable"}`), not silently ignored at read-time. This matches the existing `update_constraint` procedure's validate-before-write pattern (`validate_constraint {definition: ...} -> $valid; when valid.errors: return {status: "invalid", ...}`).
3. **Constraint-kind policies (`kind: constraint`) can only be *narrowed* by an override, never *widened*.** A lower scope may raise severity (`warning` → `error`) or add an additional `check` clause (logically AND'd), but may not lower severity or delete a check inherited from a higher, non-overridable scope. Setting-kind policies (`kind: setting`) have no such directional constraint — they are plain value overrides.
4. **Absence at all scopes falls back to the secure-default baseline** (§4), not to an undefined/null value. This makes "no config present" behaviorally identical to "the vendor default," which is the same posture OpenClaw uses out of the box (a fresh install is secure without any user-authored config).
5. Resolution and enforcement reuse `constraint-eval.px`'s existing severity dispatch verbatim: `enforce_violation` still maps `error → block`, `warning → warn`, `log → log`; only the input has an added scope-resolution step in front of it.

### 4. Secure-default set

Ships as `kind: setting` and `kind: constraint` entries under `policy:global:*` with `secure_default: true`, seeded at engine-install time (not authored per-deployment). Directional principle: **match OpenClaw's out-of-the-box posture** for each customization category rather than inventing new defaults:

- **Tool/action approval**: default `AllowWithApprovalWarning`-equivalent posture for anything with an external/public side effect (network egress beyond the local host, credential/secret access, destructive filesystem ops) — `overridable: true` per-plugin (extensions may declare narrower policies) but the *global floor* (`kind: constraint`, `severity: error`, `overridable: false`) still blocks true destructive actions (irreversible deletes, credential exfiltration) regardless of any plugin/host override, mirroring the workspace's own "ask first" external-action gate.
- **Cross-scope trust**: a plugin's settings apply only within its own `policy:plugin:<id>:*` namespace by default; a plugin cannot read or write another plugin's namespace or `policy:host:*` without an explicit `overridable: true` grant recorded at `global` scope.
- **Constraint enforcement default**: unknown/unregistered constraint names evaluate as `severity: warning` (log + warn), not silently ignored and not hard-`error` — matching OpenClaw's default of visible-but-non-blocking for unrecognized policy surface, escalate to `error` only for named, deliberately hardened constraints.
- **Audit**: every policy/setting write emits `{type: "policy.updated", scope, name, updated_by}` (same `emit` mechanism `directives.px` already uses for `routing.resolved`, `architecture.validated`, etc.) — settings changes are observable by construction, not an opt-in.

This is a starting baseline to be refined during the build stage; this ADR fixes the *categories and precedence direction*, not the exhaustive final value table.

### 5. Authoring as `.px` procedures

No new authoring surface. Policies/settings are authored exactly like `constraint-eval.px` today, generalized:

```
# policy-engine.px (illustrative — build-stage will produce the real file)

procedure resolve_policy(name: string, plugin_id: string?, host_id: string?) -> resolved:
  given: "Resolve a policy/setting by most-specific scope: host > plugin > global"
  when host_id:
    db_get {key: "policy:host:"} -> $host_entry
    when host_entry: return {source: "host", entry: $host_entry} end
  end
  when plugin_id:
    db_get {key: "policy:plugin:"} -> $plugin_entry
    when plugin_entry: return {source: "plugin", entry: $plugin_entry} end
  end
  db_get {key: "policy:global:"} -> $global_entry
  when global_entry: return {source: "global", entry: $global_entry} end
  db_get {key: "policy:secure-default:"} -> $secure_default
  return {source: "secure_default", entry: $secure_default}

procedure update_policy(scope: string, name: string, definition: string, actor: string) -> updated:
  given: "Write or override a policy/setting entry, enforcing overridable/floor rules"
  db_get {key: "policy:global:"} -> $floor
  when floor.overridable == false AND scope != "global":
    return {status: "rejected", reason: "not_overridable"}
  end
  validate_policy_definition {definition: $definition} -> $valid
  when valid.errors:
    return {status: "invalid", errors: $valid}
  end
  pluresdb_write {key: "policy::", value: $definition}
  emit {type: "policy.updated", scope: $scope, name: $name, updated_by: $actor}
  return {status: "updated"}

procedure evaluate_policy_constraints(state: string, plugin_id: string?, host_id: string?) -> evaluations:
  given: "Evaluate all applicable policy constraints (global + plugin + host) against current state"
  db_get_prefix {prefix: "policy:global:"} -> $global_policies
  db_get_prefix {prefix: "policy:plugin:"} -> $plugin_policies
  db_get_prefix {prefix: "policy:host:"} -> $host_policies
  loop over $global_policies as policy:
    resolve_policy {name: policy.name, plugin_id: $plugin_id, host_id: $host_id} -> $resolved
    evaluate_constraint {constraint_name: resolved.entry.name, state: $state} -> $result
  end
  return {results: $global_policies}
```

These procedures reuse `db_get`, `db_get_prefix`, `pluresdb_write`, `emit`, `when/end`, and `loop over ... as ... end` — every construct already present and parsed in the current `.px` procedures reviewed above. No new keyword, queue type, or storage backend is required. Enforcement continues to flow through the existing `enforce_violation` queue-driven procedure for `kind: constraint` entries; `kind: setting` entries are read-only lookups with no enforcement queue.

## Consequences

- **Positive**: Policy/settings governance becomes a natural, incremental extension of the constraint model already live in `constraint-eval.px` — no parallel config system, no new MCP tool family beyond what `radix__praxis-*` already exposes (extend with `policy-get`/`policy-set`/`policy-evaluate` verbs mapped onto the same procedures).
- **Positive**: Deferring per-host content (praxisbot/surface/kbristol-devbox) while shipping the *scope mechanism* now means hosts can each be onboarded later as pure data (new `policy:host:<id>:*` entries), with zero engine code change.
- **Risk**: The `overridable: false` floor and narrowing-only override rule (§3.2–3.3) need enforcement at write-time in `update_policy`/`update_constraint`-equivalent code; if build-stage skips that validation, the floor becomes advisory-only. Flag as a required test case for the build stage.
- **Risk**: "Match how OpenClaw behaves out of the box" (§4) is a directional principle, not a spec — build stage must enumerate the actual OpenClaw default-policy categories to diff against, or this ADR's defaults will drift from the stated intent.
- **Deferred (explicit, per 2026-07-29 decision)**: praxisbot/surface/kbristol-devbox per-host `policy:host:*` content. Do not populate these until this engine ships and the scope mechanism is proven with `global`-only policies first.

## Next steps (build stage — not this ADR)

1. Implement `policy-engine.px` for real (the procedure sketch in §5 is illustrative, to be hardened with actual queue wiring per `spine/*.px` conventions).
2. Extend `radix__praxis-*` MCP tools with `policy-get`/`policy-set`/`policy-evaluate` verbs.
3. Migrate existing flat `constraint:*` entries to `policy:global:*` with `kind: constraint` (compatibility shim during transition).
4. Write the secure-default baseline value table (§4 lists categories only).
5. Only after (1)-(4) ship: begin per-host `policy:host:{praxisbot,surface,kbristol-devbox}:*` authoring as separate, later work.
