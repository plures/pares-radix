# Roadmap — pares-radix

> Refreshed 2026-07-11. This is the strategic plan; per-item design detail lives in
> `.praxis/decisions/ADR-*`. The plugin-format foundation is **ADR-0024** (canonical
> `plugin.toml` + `.px` + adapter + `ui/`, capability dependency registration). This roadmap
> sequences the work that builds on it: mobile (iOS/Android via Rust/Tauri), design-dojo
> plugin-UI primitives, plugin dev + test tooling, agens⇄plugin interoperability (agens using
> plugins as tools/skills), and user/agens-driven plugin customization (the extensible-inventory
> exemplar).

## Vision
The Praxis base application: a plugin-driven, local-first platform where **pure decision logic
lives in `.px` (PluresDB procedures)**, **side effects live at declared adapters**, **state lives
in PluresDB**, and **UI is contributed on `@plures/design-dojo`**. Every plures domain app becomes
a plugin. Agens (the agent plugin) is a first-class plugin *consumer* — it can invoke any other
plugin's capabilities as tools, so the same customization a user does by hand, agens can do on the
user's behalf.

## Guiding invariants (do not regress)
- **One plugin format** — `plugin.toml` + `procedures/*.px` + `adapter/*.ts` + `ui/*.svelte`
  (ADR-0024). No hand-written `RadixPlugin` logic objects; `manifest.json` is generated.
- **Logic in PluresDB, IO at the boundary, state in PluresDB** (C-PLURES-003/004).
- **Cross-plugin interaction is CID-mediated** (events / PluresDB nodes), never direct refs
  (ADR-0011/0022). Agens⇄plugin is the same mediated path — no special back door.
- **No stubs** (C-NOSTUB-001). A plugin/feature is either real or absent.
- **Channel-independent QA; build the binary, run the binary** (C-TEST-001/002).

---

## Phase A — Plugin Foundation Completion (in progress)
*Goal: the canonical format is real end-to-end for one consumer and one provider, with a
contribution loader the host actually uses.*

- [x] Canonical format decided (ADR-0024) + capability host contract (ADR-0022) + observability
      event contract (ADR-0023).
- [x] `commerce` provider plugin (`plugin.toml` + `.px` + mediated adapter) as the reference.
- [ ] **`plugin.toml` contribution loader** in the host: derive routes/nav/settings/dashboard
      widgets from `[contributes.*]`, lazy-load `ui/*.svelte`, permission-gate `[permissions]`,
      resolve `[dependencies].capabilities`. Single TOML parse path (reuse ADR-0022 step-1 parser).
- [ ] **`build-registry` projection** `plugin.toml → manifest.json/index.json` in CI (C-DRIFT-001).
- [ ] **secrets@1.x provider** (port `plures-vault`) + `capabilities/secrets.cid.toml`, so
      vault/netops/financial-advisor **depend on** one secrets capability instead of each shipping
      their own store (the dedup goal of ADR-0024 §3).
- [ ] **First consumer with real `ui/`**: migrate `financial-advisor` (logic → `.px`,
      `localStorage` → `ctx.data`, pages → `ui/` on design-dojo) as the consumer exemplar.

## Phase B — design-dojo Plugin-UI Kit (enables customization + inventory)
*Goal: give plugin authors AND the schema-driven runtime the primitives an extensible,
user-customizable plugin needs — so UI is generated from a schema, not hand-built per plugin.*

Current kit (14 primitives) is layout/typography-heavy and lacks the data-heavy pieces a
domain plugin like inventory needs. Add, in priority order:

- [ ] **`DataGrid`** — sortable/filterable/paginated table bound to a PluresDB collection;
      columns derived from an entity schema (`plugin.toml [[schema.entities]]`). GUI+TUI tokens.
- [ ] **`SchemaForm` / `FormBuilder`** — renders a create/edit form from an entity schema
      (field types → inputs, validation from `.px` constraints). The write path for any
      collection. This is what makes a plugin "customizable without code."
- [ ] **`EntityList` / `DetailView`** — master/detail surfaces bound to a collection + a node.
- [ ] **`FieldEditor` / `SchemaDesigner`** — lets a user (or agens) add/rename/retype fields on a
      customizable entity at runtime, persisting the schema delta to PluresDB (see Phase E).
- [ ] **`FilterBar`, `Toolbar`, `Badge`, `Tag`, `EmptyState`** — the supporting surface a
      data-driven plugin reuses. Enforce the UX empty-state contract centrally.
- [ ] **Design-token audit** for GUI/TUI parity across all new components (the netops GUI/TUI
      parity bar). Every new primitive ships both token sets.
- [ ] **Storybook-style component gallery** (`design-dojo` gallery route) as the living catalog +
      visual regression target. Kit vs. catalog rule (ADR-0024 §5): product screens stay in plugins.

## Phase C — Mobile: Rust/Tauri for iOS & Android (new)
*Goal: the same `.px`/PluresDB core and design-dojo UI run as native iOS/Android apps via Tauri 2
mobile, with no logic fork. Design captured in ADR-0030.*

- [ ] **ADR-0030 — Mobile targets via Tauri 2 mobile** (accepted design; this roadmap item tracks
      execution). Decides: shared `radix-core` crate compiles to `aarch64-apple-ios` /
      `aarch64-linux-android`; the frontend bridge contract (navigate/window-state/tray) degrades
      gracefully on mobile (no tray; system back button → `user.navigated`).
- [ ] **`radix-core` mobile build matrix** — `cargo build` green for iOS + Android targets in CI
      (build the binary), NDK/toolchain pinned. No desktop-only syscalls in the core.
- [ ] **Tauri mobile shell** — `pnpm tauri android init` / `ios init`; wire the existing bridge
      commands; capture a real screenshot on an emulator/simulator (build-the-binary gate).
- [ ] **PluresDB on mobile** — confirm the storage adapter (sqlite/CRDT) links + syncs on both
      targets; mobile is a first-class replica, not a thin client (local-first invariant).
- [ ] **Responsive design-dojo** — the Phase-B primitives adapt to touch/narrow viewports; TUI
      tokens unaffected. Command palette → mobile action sheet.
- [ ] **Permissions/adapters on mobile** — network/notify/storage capability gates map to
      iOS/Android permission prompts at the adapter boundary (ADR-0011 on mobile).
- [ ] **Packaging** — `.ipa` / `.aab` build lanes (unsigned/dev first; signing is a later, gated,
      external-side-effect step requiring explicit approval).

## Phase D — Plugin Dev & Test Tooling (new/expanded)
*Goal: authoring, validating, and testing a plugin is fast, local, and channel-independent — so
new plugins and agens-generated plugins are trustworthy by construction.*

- [ ] **`create-radix-plugin` scaffolder** — generates `plugin.toml` + `procedures/` + `adapter/`
      + `ui/` + `tests/` from the canonical template; refuses raw HTML / TS decision logic.
- [ ] **`radix plugin validate`** — one command running the estate gates locally:
      `validate-dependencies` (every capability dep has a provider), `validate-cid-surface`
      (provider matches its CID), contract coverage, no-stub scan, drift check.
- [ ] **Plugin test harness** — load a plugin into a real headless radix instance, drive it via
      `ctx.data` + capability events, assert PluresDB state. BLOCKS for providers (ADR-0024 §6),
      strongly encouraged for consumers. Never through a chat adapter (C-TEST-002).
- [ ] **`.px` procedure unit runner** — evaluate a procedure against fixture facts and assert
      emitted events/nodes (pure-logic tests, no IO). Wire into the harness above.
- [ ] **Live-reload dev loop** — `radix plugin dev <id>` mounts the plugin in the running shell,
      hot-reloads `ui/` and re-compiles `.px` on change.
- [ ] **Author guide** (`docs/PLUGIN-AUTHOR-GUIDE.md`) kept current: canonical format, three-way
      split, capability deps, design-dojo UI, testing gate, agens-tool exposure (Phase E).

## Phase E — Agens ⇄ Plugin Interoperability (new — the headline)
*Goal: agens can discover and invoke any installed plugin's capabilities as tools/skills, and can
drive the same customization surfaces a user drives — all through the existing CID-mediated path,
with permission gating and an audit trail. Design captured in ADR-0031.*

- [ ] **ADR-0031 — Plugins-as-tools for agens** (accepted design). Decides: every plugin's
      `[capabilities.provided]` operations are auto-projected into an **agens tool registry** (the
      `crates/marketplace` skill_category surface is the seam); agens invokes a tool by emitting the
      capability's `*.requested` event and awaiting the `*.completed` event — identical to any
      consumer plugin, no privileged API. Permission gate: a plugin op is agens-invokable only if
      its `[permissions]`/trust tier allows, and each invocation is recorded in the decision ledger.
- [ ] **Tool projection** — generate agens tool descriptors from CIDs (operation name, inputs,
      outputs) so agens sees plugins the way it sees built-in tools/skills.
- [ ] **Capability discovery for agens** — agens can query "what can I do here?" → the resolver
      lists installed providers + operations (feature-detection, not a hardcoded list).
- [ ] **UI-manipulation surface** — expose design-dojo customization ops (add field, add widget,
      reorder, retheme) as mediated events so **agens can customize a plugin's UI on the user's
      behalf**, and the change persists to PluresDB like a user's manual change.
- [ ] **Interop test matrix** — end-to-end tests where agens: (a) invokes a provider op as a tool,
      (b) reads/writes a customizable plugin's collection, (c) customizes that plugin's UI/schema,
      (d) is correctly BLOCKED by a permission/trust gate. Channel-independent (C-TEST-002).

## Phase F — Extensible Inventory (the customization exemplar)
*Goal: a real inventory plugin that a user — and by extension agens — can customize: define what
kind of inventory it holds, what fields each item has, and what operations apply. Proves Phases
B/D/E together. Replaces today's stub `src/routes/inventory` maintenance page.*

- [ ] **`plugins/inventory`** as a canonical plugin (`plugin.toml` + `.px` + adapter + `ui/`),
      depends on `storage@^1.0` (and optionally `secrets` for private inventories).
- [ ] **User-defined item types** — inventory does NOT hardcode "product." An **item-type is a
      user-authored entity schema** (fields + types + `.px` validation constraints) persisted to
      PluresDB. Ships with example types (pantry, parts bin, book library, homelab assets) as
      *seed data*, not hardcoded logic.
- [ ] **Schema-driven UI** — the `DataGrid`/`SchemaForm`/`SchemaDesigner` (Phase B) render each
      item type from its schema. Adding a field in the UI updates the schema node → the grid/form
      regenerate. No per-type code.
- [ ] **Customizable operations** — check-in/check-out, quantity adjust, low-stock alert, location
      move — expressed as `.px` procedures the user can enable/parameterize per item type
      (thresholds, locations) via settings, not code.
- [ ] **Agens integration (closes the loop with Phase E)** — agens can: create a new item type
      ("track my 3D-printer filament with color, material, grams-remaining"), add items, run
      operations, and restyle the grid — all via the mediated inventory capability, recorded in the
      ledger. This is the concrete demonstration that a user's customization and agens's
      customization are the *same* mediated surface.
- [ ] **Test cases** — schema CRUD, field add/rename/retype with existing data migration, operation
      enable/param, agens-driven type creation + customization, permission gating. Build-the-binary,
      drive via the host API.

---

## Cross-cutting (carried through all phases)
- **PluresDB-backed everything** — no `localStorage`, no ad-hoc files (C-PLURES-003).
- **Decision ledger surfacing** — customization + agens actions are auditable in the UI.
- **Import/export/backup** of full app state incl. user-authored schemas (Phase A→C portability).
- **Even/odd release gating** (ADR-0029) applies to all shipped phases.

## Sequencing rationale
Phase A unblocks a real consumer + provider. Phase B (design-dojo data primitives) and Phase D
(tooling) are prerequisites for Phase F (extensible inventory) and make Phase E's UI-manipulation
possible. Phase C (mobile) is parallelizable with B/D once `radix-core` is confirmed target-clean.
Phase E is the strategic payoff (agens⇄plugin), and Phase F is its living proof. B and D can start
immediately and in parallel; C can start as soon as ADR-0030 lands; E depends on B+D; F depends on
B+D+E.
