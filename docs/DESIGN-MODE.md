# Design Mode Architecture — pares-radix

## Overview

Design Mode enables pares-radix applications to modify their own praxis rules,
UX contracts, plugin manifests, and component schemas from within the running
application. This is the self-designing capability.

## Core Concept

The application's behavior is already fully defined by praxis primitives:
- **Facts** — named state
- **Events** — triggers
- **Rules** — event→fact transformations with contracts
- **Constraints** — system invariants
- **Gates** — readiness guards

Design Mode exposes these primitives as **editable schemas** in the UI.
Changes are persisted to PluresDB and take effect immediately via the
praxis reactive engine.

## Architecture

```
┌──────────────────────────────────────────┐
│              Design Mode UI              │
│  ┌────────────┐ ┌──────────┐ ┌────────┐ │
│  │Rule Editor │ │Constraint│ │  Route  │ │
│  │            │ │  Editor  │ │ Editor  │ │
│  └─────┬──────┘ └────┬─────┘ └───┬────┘ │
│        │             │            │      │
│  ┌─────┴─────────────┴────────────┴────┐ │
│  │         Schema Registry             │ │
│  │  (design-mode.schemas fact store)   │ │
│  └─────────────────┬───────────────────┘ │
└────────────────────┼─────────────────────┘
                     │
          ┌──────────┴──────────┐
          │   Praxis Engine     │
          │   (live reload)     │
          └──────────┬──────────┘
                     │
          ┌──────────┴──────────┐
          │     PluresDB        │
          │  (persistence)      │
          └─────────────────────┘
```

## New Praxis Primitives for Design Mode

### Facts
- `design.mode.active` — boolean, whether design mode is enabled
- `design.schema.registry` — all editable schemas indexed by id
- `design.edit.active` — currently active editor (rule/constraint/route/component)
- `design.edit.draft` — unsaved changes to a schema
- `design.edit.validation` — real-time validation of the draft

### Events
- `design.mode.toggled` — user toggled design mode on/off
- `design.schema.selected` — user selected a schema to edit
- `design.schema.saved` — user committed a schema change
- `design.schema.reverted` — user discarded changes
- `design.schema.created` — new schema created (rule, constraint, etc.)
- `design.schema.deleted` — schema removed

### Rules
- `rule.design-mode-toggle` — on design.mode.toggled, update UI affordances
- `rule.schema-validation` — on design.edit.draft change, validate against contracts
- `rule.schema-apply` — on design.schema.saved, hot-reload the praxis module
- `rule.schema-audit` — log all design changes to the decision ledger

### Constraints
- `constraint.design-mode-auth` — only authorized users can enter design mode
- `constraint.schema-validity` — saved schemas must pass all contract invariants
- `constraint.no-orphan-routes` — every route must map to a plugin
- `constraint.no-broken-contracts` — every rule must maintain contract examples

## UI Components (design-dojo)

### Required New Components
1. **DesignModeToggle** — floating action button or toolbar toggle
2. **SchemaExplorer** — tree view of all praxis primitives (facts/events/rules/constraints)
3. **RuleEditor** — edit rule trigger, emits, and evaluate function with live preview
4. **ConstraintEditor** — edit constraint check function and message
5. **ContractEditor** — add/edit examples and invariants for a rule's contract
6. **RouteEditor** — edit plugin routes, data requirements, nav items
7. **ComponentPicker** — select design-dojo components for schema-driven rendering
8. **DesignModeOverlay** — highlights editable regions when design mode is active
9. **ValidationPanel** — shows real-time validation results as schemas are edited

## Implementation Phases

### Phase 1: Foundation (this sprint)
- Design mode toggle (fact + rule + UI affordance)
- Schema registry (collect all praxis modules into an editable index)
- Schema explorer (read-only tree view of all primitives)
- Persist design mode state to PluresDB

### Phase 2: Rule Editing
- Rule editor with live validation
- Contract editor for examples and invariants
- Hot-reload praxis modules on save
- Decision ledger audit trail for all changes

### Phase 3: Visual Design
- Component picker from design-dojo catalog
- Route editor for plugin manifests
- Schema-driven component rendering
- Export edited schemas as plugin manifests

### Phase 4: Self-Design Loop
- LLM-assisted schema generation (describe behavior in natural language → praxis rules)
- Inference engine integration (confidence-scored suggestions for new rules)
- Design mode accessible from Telegram (text-based schema editing)

## Key Constraint

All design mode UI MUST use design-dojo components. The design mode itself
is a praxis module — it follows the same rules it enables editing of.
This is the self-referential property: the tool that edits praxis rules
is itself governed by praxis rules.
