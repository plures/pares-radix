# DRIFT.md — Documented, intentional overrides for `packages/design-dojo/` vendored files

Per ADR-0035 §2.3, any vendored file under `packages/design-dojo/src/` that intentionally differs
from its upstream `@plures/design-dojo` counterpart (per `UPSTREAM_MAP.json`) must have an entry
here explaining why, or `scripts/check-design-dojo-drift.mjs` fails CI for that file.

Each entry must include: the reason for the override, a link to the upstream issue/PR (if any),
and an expected removal date/condition.

## GraphView.svelte

- **Reason:** The vendored shim's other local files (`types-local.ts`) currently hold all shared
  local type definitions for this package; the standalone repo instead ships `GraphView.types.ts`
  as its own module. The vendored `GraphView.svelte` therefore imports from `./types-local.js`
  instead of `./GraphView.types.js` — this is the ONLY difference from upstream as of the
  ADR-0035 pilot reconciliation (2026-07-24); everything else, including the component doc header,
  is byte-identical to `@plures/design-dojo` standalone repo HEAD `c0c7667`.
- **Upstream link:** none yet filed — tracked as part of ADR-0035 §2.2 step 5 (remaining component
  sequencing). Filing a small upstream issue to either (a) publish `GraphView.types.ts` as part of
  the npm package's public export surface so the shim can import it directly, or (b) have the shim
  package's own `types-local.ts` re-export from the npm-published types module, is the follow-up.
- **Expected removal:** once `@plures/design-dojo` npm publishes `GraphView.types.ts` importable
  from the package root, update the vendored file's import path to match and remove this entry.
  Tracked via `UPSTREAM_MAP.json` entry `GraphView.svelte.removeAfterNpmVersion = "0.18.0"`.
