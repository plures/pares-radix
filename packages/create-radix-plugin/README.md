# @plures/create-radix-plugin

Scaffolder for **canonical Radix plugins** (ADR-0024). Generates the one canonical
plugin shape — `plugin.toml` + `procedures/*.px` + `adapter/*.ts` + `ui/*.svelte` +
`tests/` + `README.md` — from the ADR-0024 template, with the mandatory three-way
split (logic in `.px`, IO in the adapter, UI on `@plures/design-dojo`) enforced by
construction.

## Why a `packages/` package (not a `scripts/` entry)

The repo is a pnpm workspace (`packages/*`) with per-package `tsx`/`vitest` tooling
(see `@plures/radix-mcp-server`). A scaffolder is a real, tested, reusable dev tool
with its own dependency (`smol-toml`) and its own test suite — it belongs alongside
the other workspace packages, matching the `create-*` npm-init idiom. `scripts/`
here holds thin shell one-offs (`test-gui.sh`, `verify-local.ps1`), not tooling with
a test gate. So: `packages/create-radix-plugin/`.

## Usage

```bash
# from the repo root (writes into ./plugins/<id> by default):
pnpm -F @plures/create-radix-plugin start my-plugin --name "My Plugin" --icon 🚀

# or directly with tsx:
tsx packages/create-radix-plugin/src/index.ts my-plugin

# flags:
#   --name "Human Name"   (default: derived from id)
#   --desc "..."          (default: generated)
#   --icon X              (default: 🧩)
#   --out <dir>           (default: <repo>/plugins)
#   --force               (overwrite an existing plugin dir)
```

Generated tree:

```
plugins/<id>/
  plugin.toml               # ADR-0024 section 2 manifest (validated)
  procedures/<id>.px        # real reactive procedure + pure rule (no placeholder)
  adapter/<id>-adapter.ts   # thin IO seam, documented boundary, no decision logic
  ui/Dashboard.svelte       # design-dojo binding (no raw decision markup)
  tests/<id>.test.ts        # vitest for the seam + the .px rule invariant
  README.md
```

## Validate

```bash
tsx packages/create-radix-plugin/src/validate.ts plugins/<id>/plugin.toml
```

Checks the `plugin.toml` against the ADR-0024 section-2 schema: `[plugin]` (+ required
fields, `trust` enum), `[capabilities.required|optional|provided]`, `[permissions]`,
`[dependencies].plugins[]`/`.capabilities[]`, `[[contributes.routes]]`,
`[[contributes.navItems]]`.

## Test

```bash
pnpm -F @plures/create-radix-plugin test
```

Runs `scaffold()` into a temp dir and asserts the canonical tree + a valid `plugin.toml`.

## References

- ADR-0024 — Canonical plugin format & capability dependency registration
- ADR-0022 — Capability host contract
- ADR-0011 — Plugin security
- `plugins/commerce/` — the reference provider plugin
