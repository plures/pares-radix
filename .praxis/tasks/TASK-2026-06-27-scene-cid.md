# TASK-2026-06-27-001 — Bring the `scene@1.x` capability CID online (ADR-0022 follow-up)

**Owner (architect):** mswork (orchestrator)
**Human gate:** kbristol
**Priority:** WORK-PRIORITIZATION "NEXT" (ADR-0022 follow-ups — next inner-space capability)
**Reference pattern:** `secrets@1.x` — `capabilities/secrets.cid.toml` + `pares-modulus/plugins/secrets-provider/` + verify via `pares-modulus/gates/validate-dependencies.ts`.

---

## Why scene (and why now)

inner-space `plugin.toml` declares `[capabilities.required] scene = "^1.0"` (3D rendering). It is a **capability consumer** with no provider — today it binds to host stand-ins only. `commerce@1.x` and `secrets@1.x` are already online the proven way (CID → provider → consumer wire → verify-on-target). `scene` is the strongest next CID because its surface is **already real and shipped in inner-space**, not invented:

- `docs/RENDER-STATE-CONTRACT.md` (31KB) — the render-agnostic state contract (source of truth)
- `src/projection/rsv-projection.js` — the projection runtime (derives RSV from CRDT merge gaps / HLC)
- `schema/render-state-view.schema.json`, `schema/intent.schema.json` — real node schemas
- `praxis/render-convergence.px`, `tests/render-contract.px` — real Praxis invariants
- supporting node schemas: `player.schema.json`, `ship.schema.json`, `colony.schema.json`, `arena.schema.json`, `physics-zone.schema.json`

C-NOSTUB-001: every node/op/event/invariant in the CID MUST cite a real `inner-space:file:symbol`. Anything not yet shipped is `deferred = true` with the absent symbol cited — never faked.

---

## Stages (gated — a stage may not start until the prior gate is GREEN)

### Stage 1 — DESIGN (analyze + ground)
- Read RENDER-STATE-CONTRACT.md, rsv-projection.js, the two core schemas, and the two .px files end-to-end.
- Extract the real surface: RSV entity node(s), Intent node(s), RendererCapabilityProfile, convergence/signature fields, the intent vocabulary (Move/Action/Trade/Claim/Attest/ScanContribution), tiers (lite/standard/premium/ambient), spatial modes (2d/3d/mesh/text).
- Produce a grounding map: every planned CID symbol → `inner-space:<file>:<symbol>`. Flag anything that would have to be invented → mark deferred or drop it.
- **GATE 1:** grounding map complete; zero un-grounded required symbols. Output the map to `/tmp/scene-cid-grounding.md`.

### Stage 2 — FIX (author the artifacts)
- `capabilities/scene.cid.toml` — the CID, mirroring `secrets.cid.toml` structure exactly: `[cid]` (name/version/title/summary/source_repo/source_paths/foundation_reuse), `[[nodes]]`, `[[operations]]` (mediated request/result events, ADR-0011), `[events]`, `[transport]`, `[invariants]`. Every entry cites its grounding. Deferred surface honest.
- A real **scene provider** plugin following the `secrets-provider` layout (`pares-modulus/plugins/scene-provider/`: manifest.json declaring `[capabilities.provides] scene`, package.json, tsconfig, vitest.config, src/{index,provider,test-context}.ts + provider.test.ts). The provider implements the non-deferred surface for real (projection read-model + intent write-model conformance), no stubs.
- Confirm inner-space consumer already declares `capabilities.requires.scene` (it declares `scene = "^1.0"`); if the manifest uses an older key shape than the gate expects, align it (consumer side) — do NOT weaken the gate.
- **GATE 2:** `scene.cid.toml` parses; provider package builds (tsc) and `vitest run` is green; no `todo!()`/stub/mock-in-runtime.

### Stage 3 — QA / VERIFY-ON-TARGET (the mandatory gate)
- Run `pares-modulus/gates/validate-dependencies.ts` end-to-end: a consumer declaring `scene` RESOLVES against the new provider → **exit 0**; a deliberately-broken/absent requirement → **BITE exit 1**. Both directions proven.
- Run the provider's own `vitest run` from a clean build. Run any scene/render conformance .px the repo exposes.
- **GATE 3:** RESOLVE exit 0 AND BITE exit 1, both captured. Provider tests green from scratch.

### Stage 4 — DOCUMENT + COMMIT + PUSH
- Update `development-guide/practices/WORK-PRIORITIZATION.md`: mark scene CID brought online; set next remaining inner-space capability.
- Commit the CID + provider + any consumer alignment with a clear message; push pares-radix and (separately) pares-modulus / development-guide as touched.
- Close-the-loop: fresh `repo-state.ps1 -Fetch` shows all touched repos `0/0` clean.

---

## Hard rules
- C-NOSTUB-001 (no stubs, deferred-is-honest), C-PLURES-003/004 (state in PluresDB, mediated boundary), ADR-0011 (no direct fn refs across plugin boundary), ADR-0022 (CID contract), C-TEST-002 (verify via the gate, not a channel adapter).
- Build the binary, run the binary: provider tests must run from a clean build, not just typecheck.
- Verify-on-target (Stage 3) is NOT skippable. A green typecheck is not a pass.
