# ADR-0031: Plugins-as-Tools for Agens — Mediated Interoperability & User/Agent-Parity Customization

- **Status:** Accepted (DESIGN stage; projection/harness/verify stages follow)
- **Date:** 2026-07-11
- **Deciders:** kbristol (strategic directive), dev-lead orchestrator
- **Relates:** ADR-0010 (agens-first), ADR-0011 (plugin security/trust), ADR-0022 (capability host contract / CIDs), ADR-0023 (procedure observability), ADR-0024 (canonical plugin format), ROADMAP Phases E & F
- **Invariants:** C-PLURES-003/004, C-NOSTUB-001, C-TEST-001/002

---

## 1. Context

Agens is "the first plugin" — the meta-plugin that builds and drives other plugins (ADR-0010). But
there is no defined way for agens to **use** an installed plugin the way it uses a built-in tool or
skill, and no defined way for agens to perform the **same customization a user performs by hand**.
The user's directive: plugins should be usable by agens as skills/tools, and a plugin like
inventory should be extensible so that the *user, and by extension agens*, can customize its UI and
operations (e.g. store and manage different kinds of inventory).

The critical design constraint is that this MUST NOT create a privileged agens back door. ADR-0022
already defines the mediated path: a consumer interacts with a provider only through the CID's
`*.requested` → `*.completed` events / PluresDB nodes, permission- and trust-gated (ADR-0011),
observable (ADR-0023). Agens is just another consumer. The insight of this ADR: **the agens tool
registry is a projection of installed plugins' CIDs**, and **UI/schema customization is itself a
set of mediated capability operations** — so a user's manual customization and an agens-driven
customization travel the exact same path and land as the same PluresDB state.

`crates/marketplace` already has a `skill_category` surface; that is the seam where plugin
operations become agens-visible skills/tools.

## 2. Decision

1. **Tool registry = CID projection.** For every installed plugin, each operation in its
   `[capabilities.provided]` / CID `[[operations]]` is auto-projected into an **agens tool
   descriptor** (name, input schema, output schema, description) via the marketplace
   `skill_category` seam. Agens sees plugin operations the way it sees built-in tools/skills. The
   projection is generated from the CID, not hand-authored (C-DRIFT-001) — a new provider becomes
   agens-usable with zero extra code.

2. **Invocation is the mediated path, unchanged.** Agens invokes a tool by emitting the operation's
   `*.requested` event (with inputs conforming to the CID) and awaiting the `*.completed` event —
   **identical to any consumer plugin**. No direct function refs, no privileged API. This makes
   every agens action auditable and CRDT-native by construction.

3. **Permission + trust gating.** An operation is agens-invokable only if the plugin's
   `[permissions]` and trust tier (ADR-0011) permit it for the agent principal. Denials are real,
   handled results (not silent). This is the enforcement point that keeps "agens can use plugins"
   from becoming "agens can do anything."

4. **Every agens invocation is ledgered.** Each tool call is recorded in the decision ledger
   (ADR-0023 observability) with inputs summary, outcome, and the gate decision — so the user can
   see and audit what agens did through plugins, in the UI.

5. **Capability discovery.** Agens can query "what can I do in this context?" and the ADR-0022
   resolver returns installed providers + operations (feature-detection). Agens never assumes a
   hardcoded tool list; absent capability → the tool simply isn't offered (honest absence,
   C-NOSTUB-001).

6. **Customization is a capability, not a special mode.** UI/schema customization operations —
   define/alter an entity schema (add/rename/retype field), add/reorder/retheme a
   contributed widget, enable/parameterize an operation — are exposed as **mediated capability
   operations** on the customizable plugin. Therefore:
   - A **user** customizes via a design-dojo surface (SchemaDesigner/FieldEditor, Phase B) that
     emits those operations.
   - **Agens** customizes by emitting the *same* operations as tool calls.
   - Both persist to PluresDB as the same schema/contribution nodes. **User-parity is structural,
     not simulated** — there is one write path, two callers.

7. **The extensible-inventory exemplar (ADR-0024 §7 / ROADMAP Phase F) is the acceptance target.**
   Inventory exposes `define_item_type`, `add_field`, `add_item`, `run_operation`,
   `customize_view` as CID operations. The user creates a "3D-printer filament" item type with
   color/material/grams fields; agens can create the identical type via tool calls; both yield the
   same PluresDB schema. That equivalence is the proof this ADR works.

## 3. What this explicitly forbids
- A privileged agens-only API that bypasses CID mediation or permission gates.
- Hardcoded per-plugin tool lists in agens (must be projected from CIDs).
- Any customization write path that differs between "user did it" and "agens did it."
- Unledgered agens plugin invocations.
- Stubbed tool projections (a projected tool must really invoke the real operation, or not be
  offered — C-NOSTUB-001).

## 4. Consequences

**Positive** — plugins become agens's skill/tool surface for free; agens and the user share one
customization path; everything agens does through plugins is gated + audited; works identically on
desktop and mobile (ADR-0030) because it rides the same events + state.

**Costs/risks** — the CID→tool-descriptor projection must stay in sync with CIDs (mitigation:
generate in CI, gate on drift); permission gating for a non-human principal needs a clear policy
(mitigation: reuse ADR-0011 trust tiers, deny-by-default for anything not explicitly permitted);
schema-mutation operations must migrate existing data safely (mitigation: field add/rename/retype
operations carry a `.px` migration rule, tested against existing rows).

## 5. Acceptance criteria (verify stage — channel-independent, build-the-binary)
- A newly installed provider's operations appear as agens tools with no code change.
- Agens invokes a provider operation via `*.requested`/`*.completed` and gets the same result a
  consumer plugin would; the call is ledgered.
- A permission/trust-denied operation is BLOCKED for agens with a real handled result.
- Agens creates an inventory item type + fields via tool calls; the resulting PluresDB schema is
  byte-equivalent to a user creating it via the design-dojo surface.
- Field add/rename/retype migrates existing inventory rows correctly.
- All tests drive the host API / capability events directly — never a chat adapter (C-TEST-002).

## 6. References
ADR-0010, ADR-0011, ADR-0022, ADR-0023, ADR-0024; ROADMAP Phases E & F;
`crates/marketplace` (`skill_category`); C-PLURES-003/004, C-NOSTUB-001, C-TEST-001/002,
C-DRIFT-001.
