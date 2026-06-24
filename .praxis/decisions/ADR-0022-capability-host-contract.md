# ADR-0022: Capability Host Contract (Provider Capabilities)

## Status: Accepted

## Date: 2026-06-23

> **Implementation status (updated 2026-06-23): ALL 6 STEPS IMPLEMENTED & PUSHED.** This
> design is fully realized in code; the loader, CID type, first CID, the real provider
> plugin, the inner-space binding proof, and the modulus CI gate are all landed and gate-green
> (re-verified from scratch by the architect, not subagent self-report). Commit map is in the
> Implementation outline below. Remaining honest follow-ups are noted per step (none block the
> contract).

## Context

The whole purpose of pares-radix is to be an **app framework that hosts plures-org
ideas built in praxis logic/primitives without duplicating the Tauri/Rust shell for
every new app**. Once one game (inner-space) runs as a plugin, many more games — and
non-game apps — follow as plugins on the same shell. (Directive: kbristol, 2026-06-23.)

Today the architecture has drifted away from that goal:

- **inner-space** was already authored as a Radix plugin. Its `plugin.toml` declares
  `[capabilities.required]`: `scanning, scene, physics, audio, input, location, commerce, network`
  and `[capabilities.optional]`: `ar, notify, media`. Its logic is pure `.px`
  (`economy`, `crafting`, `fleet-ai`, `ground-ai`, `hazards`, `territory`). It is a
  **capability consumer** — it cannot run until a host *provides* those capabilities.
  **Nothing provides them yet.**

- **OASIS** shipped G1/G2/G3 as standalone TS packages **plus its own Tauri app, its own
  crypto crate, and its own praxis runner** (`apps/tauri-app`, `crates/oasis-crypto`,
  `praxis/oasis-loop.px`). That is exactly the per-app shell duplication this platform
  exists to eliminate. OASIS's reusable primitives (ZK commerce/redemption, preference
  engine, privacy-policy engine) are broadly applicable and belong in the host as
  capability providers — not trapped behind a second app binary.

- **ADR-0011 (Plugin Security)** already defines a capability model, but it scopes
  "capability" to **platform-mediated I/O adapters** with permission gating:
  `Network, Storage, UI, Notifications, LLM, System`. That is the *permission* axis.
  It does **not** model `scanning`/`scene`/`physics`/`commerce` — those are not I/O
  permissions, they are **versioned interfaces with swappable implementations**.
  inner-space's `commerce = "^1.0"` is an **interface contract** (semver-range against a
  provider), not a permission grant.

The missing Level-1 (foundation/architecture) piece is therefore a **Capability Host
Contract**: a way for plugins to both **require** and **provide** versioned capability
interfaces, and for the host to **resolve** a consumer's `[capabilities.required]`
against registered providers at install/activation time.

This ADR extends ADR-0011 and ADR-0010; it does not replace them.

## Decision

### 1. Two kinds of capability

Capabilities split into two orthogonal kinds. Both are declared in the plugin manifest;
they are resolved by different machinery.

| Kind | Examples | Who provides it | Axis | Governed by |
|------|----------|-----------------|------|-------------|
| **Platform capability** | `network`, `storage`, `ui`, `notify`, `llm`, `system` | The host, always | **Permission** (allow/deny, scoped) | ADR-0011 |
| **Provider capability** | `commerce`, `scene`, `physics`, `scanning`, `audio`, `input`, `location`, `ar`, `media` | A **provider plugin** (or a host built-in) | **Interface** (versioned contract, swappable impl) | **This ADR** |

- A **consumer** plugin lists provider capabilities under `[capabilities.required]`
  / `[capabilities.optional]` with a **semver range** (`commerce = "^1.0"`).
- A **provider** plugin lists the capabilities it implements under
  `[capabilities.provided]` with a **concrete version** (`commerce = "1.2.0"`).
- A plugin may be both (provide some, require others).

Provider capabilities are still subject to ADR-0011: a provider that performs I/O to
satisfy a capability (e.g. `network` for P2P, `system` for LiDAR) must declare those
**platform** capabilities and pass the same permission gate. Provider != privilege
escape.

### 2. Capability interface = versioned, declarative contract

Each provider capability is defined by a **Capability Interface Descriptor (CID)**: a
named, semver'd contract that specifies the surface a provider must implement and a
consumer may call. The CID is declarative (praxis-first): it enumerates the
PluresDB node types, events, and procedure entry points that constitute the interface.

A capability is **not** an arbitrary JS object handed across plugins. Per ADR-0011,
cross-plugin interaction is mediated by the platform: a consumer **emits events** /
**writes PluresDB nodes** defined by the CID; the provider's reactive procedures react;
results return as events/nodes the consumer reads. No direct function references cross
the plugin boundary. This keeps the boundary inspectable, auditable, and CRDT-native
(C-PLURES-003/004).

CID identity: `name@semver` (e.g. `commerce@1.2.0`). Backwards-compatibility rules
follow semver: a provider satisfies a consumer's range iff `provider.version ∈ range`.

### 3. Resolution at install/activation (extends the loader)

The plugin loader already does topological dependency resolution (Kahn's algorithm) over
explicit `[dependencies].plugins`. Capability resolution is an **additional edge source**
in the same graph:

```
For each consumer C and each required capability cap@range in C.required:
    candidates = { P in registered plugins : cap@v in P.provided, v in range }
    if candidates is empty        -> UNSATISFIED  (block activation, actionable error)
    if |candidates| == 1          -> bind C.cap -> that provider
    if |candidates| >  1          -> resolve by binding policy (sec. 4); else prompt
Add edge provider -> consumer (provider activates first)
Detect cycles across the combined (deps + capability) graph -> reject (as today)
```

- **Optional** capabilities that are unsatisfied do **not** block activation; the
  consumer observes them as absent (feature-detect via `ctx.capabilities.has("ar")`).
- Resolution is **deterministic and inspectable**: the binding set is written to
  PluresDB (`radix:capability:bindings:*`) and shown to the user, like the manifest
  permission prompt.

### 4. Binding selection policy (multiple providers)

When more than one provider satisfies a range, selection is deterministic:

1. **Pinned binding** — a user/config pin (`radix:capability:pin:commerce = <providerId>`) wins.
2. **Highest compatible version** within the range.
3. **Trust tier** (ADR-0011 §5): Verified > Community > Local as a tiebreak.
4. Still ambiguous → **prompt the user** (one-time), then persist the pin.

No silent nondeterministic binding. The chosen provider is recorded and auditable.

### 5. Host built-in providers vs. plugin providers

A capability may be satisfied by either:

- a **host built-in** provider (compiled into the Radix shell — e.g. `scene`/`physics`
  backed by the Tauri/render/physics crates), or
- a **provider plugin** (e.g. `commerce` provided by the ported OASIS ZK-commerce plugin).

Both register the same way (`[capabilities.provided]` with a CID version). Built-ins are
just providers that ship in-box and are always present. This lets capabilities migrate
from built-in to plugin (or back) without consumers changing — they bind to `cap@range`,
not to an implementation.

### 6. `plugin.toml` schema additions

Add `[capabilities.provided]` (consumers already use `required`/`optional`):

```toml
# Consumer (inner-space) — unchanged, already valid:
[capabilities.required]
scene = "^1.0"
physics = "^1.0"
commerce = "^1.0"
network = "^1.0"     # platform capability (ADR-0011 permission), also listed here

[capabilities.optional]
ar = "^1.0"

# Provider (ported OASIS commerce) — NEW block:
[capabilities.provided]
commerce = "1.2.0"   # CID version this plugin implements

[capabilities.interface.commerce]   # optional: point at the CID this satisfies
cid = "commerce@1.x"
spec = "capabilities/commerce.cid.px"
```

Platform capabilities (`network`, `storage`, `system`, …) listed under `required` are
routed to the ADR-0011 permission gate; provider capabilities are routed to resolution.
The loader distinguishes them by a **registry of known platform capabilities** (closed
set, host-owned); everything else is a provider capability (open set).

### 7. CID location, format, and ownership

Capability Interface Descriptors are **org-level contracts**, not owned by any single
consumer. They live in the registry/host so multiple plugins can target the same
interface.

**Format (v1): TOML-declared, not `.px`.** The `.px` grammar at the pinned foundation rev
(`pluresdb-px` rev `195c67b`) has top-level constructs `import, entity, config, fact,
rule, constraint, contract, function, trigger, scenario, procedure` (verified against
`crates/pluresdb-px/src/px/grammar.pest`). **It has no `capability` construct.** Under
C-NOSTUB-001 we do not fake a `.px`-native CID against a keyword that does not exist.
Therefore in v1 a CID is a **structured descriptor declared in the host/registry** and
referenced from `plugin.toml` via `[capabilities.interface.<name>]` (`cid = "commerce@1.x"`,
`spec = "capabilities/commerce.cid.toml"`). The descriptor enumerates the interface
surface (PluresDB node types, events, procedure entry points) as data the loader can
validate a provider against.

- Canonical CIDs ship in **pares-radix** under `capabilities/<name>.cid.toml` and are
  mirrored/indexed by **pares-modulus** for discovery.
- A provider plugin references the CID it implements; the loader validates the provider's
  declared surface against the CID at install (missing required nodes/events/procedures →
  reject, like manifest schema validation today).

**Deferred (foundation work, tracked, not stubbed): `.px`-native CIDs.** When
`pluresdb-px` grows a real `capability` grammar construct + compiler emission of
`px:capability/*` records, radix adds a `load_px_capabilities` loader (sibling of
`load_px_procedures`) and CIDs may be authored in `.px`. Until that real construct exists
upstream, the `.px`-CID path is **absent**, not stubbed. This is a separate piece of work
in the foundation repo.

## Consequences

**Positive**
- One Tauri/Rust shell (Radix) hosts many apps/games. No per-app shell duplication —
  the platform's reason to exist is satisfied (eliminates the OASIS-rebuilt-the-host
  anti-pattern).
- inner-space becomes runnable as the **first capability consumer** the moment its
  required capabilities have providers — no inner-space code change required (its
  `plugin.toml` is already correct).
- OASIS's reusable primitives (ZK commerce, preference, privacy) graduate into Radix as
  **capability providers**; OASIS remains the *hub/origin* of the marketplace idea but
  stops shipping a second binary.
- Capabilities are swappable by interface: an implementation can move built-in ↔ plugin
  without breaking consumers.
- Fully consistent with ADR-0011 (cross-plugin via mediated events/PluresDB, no direct
  refs) and C-PLURES-003/004 (state in PluresDB). CIDs are declarative data validated at
  install; the `.px`-native CID form is deferred to a real upstream grammar construct.

**Negative / costs**
- Adds a second resolution pass (capability edges) to the loader and a CID validation
  step at install — more loader surface to test.
- CIDs are now versioned contracts the org must maintain and evolve under semver;
  breaking a CID is a coordinated, breaking change.
- Mediated (event/node) capability calls add latency vs. direct calls — acceptable and
  required by the security model.

**Risks**
- CID drift between the canonical descriptor and provider implementations (mitigation:
  install-time validation of provider surface against the CID; CI gate in modulus).
- Ambiguous multi-provider binding surprising the user (mitigation: deterministic policy
  in §4 + persisted, auditable pins).
- A capability that *looks* like an interface but is really privileged I/O sneaking past
  the permission gate (mitigation: §1 — providers performing I/O must still declare the
  ADR-0011 platform capabilities; the known-platform-capability registry is host-owned
  and closed).

## Implementation outline (becomes lifecycle work, gated)

The lifecycle (analyze → fix/build → test → deploy → verify) drives each of these as
staged subagents; this ADR is the design (Pillar 1) only.

**Implementation map (all landed 2026-06-23, gates re-verified by architect):**
- Steps 1–3 — pares-radix `38f53f3` (manifest/schema + single parse path / C-DRIFT-001 fix; CID
  type + loader + resolver into the Kahn topo-sort + bindings persisted to PluresDB;
  `commerce@1.x` CID authored). Loader/resolver tests green.
- Step 4 — pares-radix `27f8a99` (provider plugin `plugins/commerce/` + `validate_provider_surface`)
  + OASIS `6906661` (real IO actor wrapping RedemptionProtocol/nullifier store + mediated e2e).
  Gate: 20 capability tests + 6 mediated e2e (issue→authorize→E004 double-spend, tier boundary,
  nullifier membership). A real `check_nullifier` scope bug (global vs campaignId) was caught & fixed.
- Step 5 — pares-radix `ccee72a` (inner-space's exact 8-cap required map binds to the REAL on-disk
  `plugins/commerce/plugin.toml` via real `parse_manifest`, validates against the real CID; negative
  `^2.0`-does-not-bind gate; provider drift guard). Gate: `plugins::capability` 24 passed.
- Step 6 — pares-modulus `fae3f8f` (`gates/validate-cid-surface.ts` blocking CI gate + vendored
  dependency-free TOML reader + PASS/FAIL fixtures, wired into `plugin-gate.yml`). Gate: PASS exit 0,
  FAIL exit 1 naming missing op/event, non-provider skip exit 0, build-registry green.

**Honest follow-ups left ABSENT (none block the contract):**
- Step 5: the consumer side is reconstructed from inner-space's required map for portability (no
  absolute cross-repo path read); provider side IS drift-guarded but the consumer fixture is not
  bound to inner-space's actual `plugin.toml`. Closing it needs a vendored manifest copy or a
  two-repo CI checkout. The 6 non-commerce required caps bind to trivial host stand-ins only so
  resolution completes (no CIDs exist for them yet) — only `commerce` is a real provider binding.
- Step 6: bonus `registry/schema.json` capability fields intentionally NOT added (avoid risk to
  existing manifests); the vendored TOML subset excludes inline tables/dates/multi-line basic
  strings (throws rather than mis-parses).
- @oasis/crypto-verification is NOT yet a pares-radix workspace dep, so the commerce IO actor +
  mediated e2e live in the OASIS repo; the radix plugin holds the declaration + a pointer. Wiring
  the dep so the e2e can live under `plugins/commerce/` is real follow-up.

1. **Manifest/schema**: add `[capabilities.required/optional/provided]` +
   `[capabilities.interface.*]` to the manifest parser; add the closed
   **known-platform-capability** registry. **Also fix the pre-existing C-DRIFT-001 bug**:
   two divergent TOML parsers (`manifest.rs::parse_manifest`, `runtime.rs::parse_toml_manifest`)
   that both silently drop `dependencies` — collapse to one parse path so capability fields
   (and deps) can't diverge.
2. **CID type + loader**: define the Capability Interface Descriptor (TOML-declared data,
   semver-versioned) + a resolver; add capability-edge resolution into the existing Kahn
   topo-sort, binding-selection policy, bindings persisted to PluresDB
   (`radix:capability:bindings:*`), unsatisfied-required → actionable block. (No
   `.px`-native CID loader in v1 — that construct does not exist upstream yet.)
3. **`commerce@1.x` CID**: author the first CID (drawn from OASIS G2 redemption/coupon/
   nullifier surface) as the proving capability.
4. **OASIS commerce → provider plugin**: port the reusable ZK-commerce logic into a Radix
   provider plugin declaring `commerce = "1.x"`; register it in modulus.
5. **inner-space as first consumer**: stand it up against **real** providers only. Per
   C-NOSTUB-001 there are NO stub providers: a capability is bound only if a real provider
   services its CID. Required capabilities without a real provider leave inner-space
   **activation-blocked with an actionable error** (not hollow-activated); optional ones
   are feature-detected absent. `commerce` is proven end-to-end against the real ported
   OASIS provider; `scene`/`physics`/etc. come online only as real providers land.
6. **Author guide + modulus gates**: document provider capabilities; add a modulus gate
   that validates provider surface against the referenced CID.

## References

- ADR-0010 — Agens-first plugin model
- ADR-0011 — Plugin security model (platform capabilities, mediated I/O, trust tiers)
- inner-space `plugin.toml` — pre-existing capability-consumer manifest
- PLUGIN-AUTHOR-GUIDE.md — author/register/install lifecycle, two-manifest model
- C-PLURES-003 / C-PLURES-004 — all state through PluresDB; pure logic in PluresDB, IO at the boundary
- C-NOSTUB-001 — no stubs anywhere; capabilities bind to real providers only, else genuinely unsatisfied
