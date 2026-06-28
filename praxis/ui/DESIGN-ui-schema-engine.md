# UI Schema & Reactive Best-Practice Engine — Design

> Status: **v1 BUILT & GREEN** (kbristol approved 2026-06-27). 98/98 tests, 0 type errors.
> Supersedes the narrow "responsive layout" idea by generalizing it.
> Author: mswork · 2026-06-27

## 1. The insight

A **UI design best practice** is, in every case we care about, the same shape:

> **A rule about the values of a known set of attributes on a known set of elements.**

Responsive layout is *one* instance of this (rules over box-model attributes, keyed on
viewport). Contrast, density, focus order, state-feedback, truncation are others. They
differ only in **which attributes** they touch and **what triggers re-evaluation**.

Therefore the foundation is not "a layout container" and not "a linter." It is:

1. A **UI Schema** — a typed, closed vocabulary of *element kinds* × *attributes*.
2. A **rule engine** over that schema with **two output modes**:
   - **validate** → emit a violation (the linter we already built), OR
   - **resolve** → write a concrete attribute value back (the reactive transform).
3. **Reactive execution** — resolved attributes are written to PluresDB; Unum re-renders.
   Triggers are PluresDB facts (`ui:viewport`, `ui:theme`, `ui:density`, …).

The layout container becomes *just one element kind* in the schema (a `container` whose
concern is box-model + position). Nothing special-cased.

This is the CSS property model, but **declarative, typed, stored in PluresDB, and
reactive** — instead of imperative stylesheets evaluated in the browser.

## 2. Why this fits the foundation (C-PLURES-004)

- The rule **logic** is pure: `(facts, attributes) → violations | resolvedAttributes`.
  It compiles to Praxis procedures/constraints in PluresDB. No side effects.
- The **only** side effect is the viewport/theme/density *bridge* at the edge (a resize
  / matchMedia listener that writes a fact). That is legitimately outside PluresDB.
- A write (a fact change) **causes** reactive procedure execution that rewrites attribute
  values. That is the spine, verbatim.
- Source vs derived is respected: authored tree (`canvas:tree`, intent intact) stays
  pristine; the resolved tree (`canvas:tree:resolved`) is derived, never hand-edited,
  always regenerated. (C-DRIFT-001.)

## 3. The UI Schema (derived from the real registry — honesty invariant)

Every attribute below maps to a prop that an existing design-dojo component **actually
exposes today** (verified against `registry.ts`). No invented attributes. When a new
component is added, the schema extends; a rule may only reference attributes some element
declares.

### 3.1 Element kinds (grouping of registered components)

| Schema kind   | Registered components                         | Primary concern                        |
|---------------|-----------------------------------------------|----------------------------------------|
| `container`   | Box, PluginContentArea, DashboardGrid         | box-model, position, flow              |
| `text`        | Text, Heading, CodeBlock                       | typography, color (contrast)           |
| `control`     | Button, Input, TextArea, Select               | state, labeling, affordance            |
| `media`       | (Image — when added)                           | alt, intrinsic size                    |
| `navigation`  | Link, Sidebar, CommandPalette                  | target, label, current-state           |
| `group`       | List, ListItem, Table                          | structure, semantics                   |
| `feedback`    | Dialog, StatusBar                              | actionability, visibility              |

> Mapping lives in data, not code branches: each registered component declares its
> `schemaKind`. (Added to `ComponentMeta`, optional, defaulted by category.)

### 3.2 Attribute groups (the closed vocabulary)

Each attribute notes the real prop(s) it derives from and whether it is **responsive**
(resolves per breakpoint) and/or **rule-targetable** (a best practice may read/write it).

**Box model / layout** (on `container`, some on all)
| Attribute   | From prop(s)            | Responsive | Notes                          |
|-------------|-------------------------|:----------:|--------------------------------|
| `direction` | Box.direction           | ✅         | row/column                     |
| `padding`   | Box.padding             | ✅         |                                |
| `gap`       | Box.gap                 | ✅         |                                |
| `align`     | Box.align               | ✅         | align-items                    |
| `justify`   | Box.justify             | ✅         | justify-content                |
| `wrap`      | Box.wrap                | ✅         |                                |
| `columns`   | DashboardGrid (derived) | ✅         | grid reflow                    |
| `hidden`    | (new, generic)          | ✅         | show/hide per breakpoint       |

**Typography** (on `text`)
| Attribute  | From prop(s) | Responsive | Notes                |
|------------|--------------|:----------:|----------------------|
| `size`     | Text.size    | ✅         | font-size            |
| `weight`   | Text.weight  | ❌         |                      |
| `color`    | Text.color   | (theme)    | contrast input       |
| `truncate` | Text.truncate / Text.maxLines | ✅ | overflow control |
| `level`    | Heading.level| ❌         | hierarchy (validate) |

**State / control** (on `control`)
| Attribute  | From prop(s)     | Responsive | Notes                       |
|------------|------------------|:----------:|-----------------------------|
| `label`    | *.label          | ❌         | accessible name (validate)  |
| `disabled` | *.disabled       | ❌         | must drive visible state    |
| `required` | Input.required   | ❌         |                             |
| `error`    | Input/TextArea.error | ❌     | must be announced           |
| `variant`  | Button.variant   | ❌         | danger ⇒ confirm (validate) |

**Color / theme** (cross-cutting)
| Attribute     | From prop(s)        | Trigger | Notes                         |
|---------------|---------------------|---------|-------------------------------|
| `color`       | Text.color          | theme   | contrast(fg,bg) ≥ 4.5 (AA)    |
| `background`  | Box (themed)        | theme   | contrast pair                 |

### 3.3 Trigger facts (what makes rules re-resolve)

| Fact key       | Shape                                            | Drives                         |
|----------------|--------------------------------------------------|--------------------------------|
| `ui:viewport`  | `{ width, height, breakpoint }`                  | layout/responsive attributes   |
| `ui:theme`     | `{ name, mode: 'light'|'dark', tokens }`         | color/contrast attributes      |
| `ui:density`   | `{ level: 'compact'|'comfortable'|'spacious' }`  | padding/gap/size attributes    |

Breakpoint ladder (standard): `base · sm 640 · md 768 · lg 1024 · xl 1280`.

## 4. Rule model — one language, two modes

A best practice is authored once against the schema:

```
practice <name>:
  kind: <validate | resolve>
  appliesTo: <schemaKind>            # e.g. container, text, control
  when: <flat boolean over facts + node attributes>
  # validate mode:
  require: <flat boolean>            # violation if false
  severity: <error | warning>
  message: <string>
  # resolve mode:
  set: <attribute> = <expression over facts + responsive map>
```

- **validate** practices are exactly today's `constraint` blocks (already shipped &
  tested). They produce `[ui:error]/[ui:warn]` issues.
- **resolve** practices are new. They compute an attribute value from (a) the node's
  responsive map for that attribute and (b) the active trigger fact, then write it to the
  resolved tree.

> The evaluator surface stays **flat boolean / arithmetic** (the proven `simpleEval`
> contract). `resolve` adds a tiny, equally-flat *expression* form for the right-hand
> side (pick-by-breakpoint, clamp, token lookup) — no tree walking, no function calls in
> author space. The breakpoint pick is a table lookup the extractor pre-flattens.

### 4.1 Responsive intent on a node (author-facing)

Optional. Zero intent = type-based defaults still apply.

```jsonc
{
  "id": "form", "type": "Box",
  "props": { "direction": "row", "gap": "16px" },
  "responsive": {
    "direction": { "base": "column", "md": "row" },
    "gap":       { "base": "8px",    "md": "16px" }
  }
}
```

Resolver collapses `responsive.<attr>` to a concrete `props.<attr>` for the active
breakpoint, writing the result to the **resolved** tree only.

## 5. Data flow (reactive)

```
resize / matchMedia (edge)         theme toggle (edge)        density setting (edge)
        │                                  │                          │
        ▼                                  ▼                          ▼
   put ui:viewport                   put ui:theme               put ui:density
        └───────────────┬──────────────────┴──────────────┬─────────┘
                        ▼ (reactive-graph subscribePrefix "ui:")
              resolveUiTree(authoredTree, facts)      ← PURE function
                        │
                        ▼
              put canvas:tree:resolved   ← derived artifact (never authored)
                        │
                        ▼
                 Unum reads :resolved → Svelte renders already-correct UI
```

`validate` practices run in the same pass (or in `canvas.validate`) and surface issues;
they never mutate the tree.

## 6. What already exists vs. what's new

**Exists (reuse):**
- `reactive-graph.ts` — `subscribe` / `subscribePrefix` / `put`. The reactive hook. ✅
- `registry.ts` — every component + its real props (schema source of truth). ✅
- `ui-facts.ts` + `ui-constraints.ts` + `.px` — the **validate** half, tested (56 green). ✅
- `simpleEval` flat-boolean evaluator + its test. ✅

**New (to build, in order):**
1. ~~`ui-schema.ts`~~ ✅ BUILT — element kinds, `RESPONSIVE_ATTRS`, breakpoint ladder,
   `kindForComponent` (category-inferred + `schemaKind` override on `ComponentMeta`).
2. ~~`responsive` field on `CanvasNode`~~ ✅ BUILT (optional, non-breaking) + `hidden`
   added to the responsive attribute vocabulary.
3. ~~`resolveUiTree(tree, facts)`~~ ✅ BUILT — pure resolver, generic interpreter of
   `UI_PRACTICES` (no per-component branches); authored tree provably untouched.
4. ~~`praxis/ui/ui-layout.px`~~ ✅ BUILT (+ TS mirror `ui-practices.ts`, + drift guard
   `ui-practices.sync.test.ts`).
5. ~~Viewport bridge~~ ✅ BUILT (`ui-viewport-bridge.ts`) — the single IO edge, SSR-safe.
6. ~~Reactive wiring~~ ✅ BUILT (`ui-reactive.ts`) — `wireResolvedTree` via
   `subscribePrefix('ui:')` + `subscribe('canvas:tree')` → writes `canvas:tree:resolved`.

## 7. Open decisions (need kbristol)

1. **Resolved tree location** — derived `canvas:tree:resolved` (recommended; pristine
   source) vs in-place mutation (lossy). *Leaning derived.*
2. **Scope of first cut** — do we ship **layout/responsive** as the first `resolve`
   practice set (proves the reactive half end-to-end), then add theme/contrast + density
   as follow-on practice files on the same engine? *Leaning yes — vertical slice first.*
3. **New attributes** — add `hidden` (responsive show/hide) and `maxLines` (truncation) to
   Box/Text now, or stay strictly within current props for v1? *Leaning add `hidden`; it's
   the most-used responsive primitive and trivial.*
4. **Schema kind source** — infer `schemaKind` from existing `category` (zero edits) vs
   add explicit `schemaKind` to each `registerComponent` (clearer, small edit). *Leaning
   infer-with-override: default from category, allow explicit.*

## 8. Non-goals (v1)

- No arbitrary CSS. The schema is a *curated* attribute set; that curation **is** the
  best-practice guardrail.
- No author-space functions/tree-walking. Flat evaluator surface preserved.
- No per-component rule code. Rules target *schema kinds*, not component names.

## 9. How to use (v1, shipped)

```ts
import {
  createReactiveGraph, wireResolvedTree, attachViewportBridge,
  resolveUiTree, // pure, for tests / SSR one-shot
} from '@plures/canvas-runtime';

// 1. Wrap your PluresDB graph and wire reactive resolution once at app start:
const graph = createReactiveGraph(baseGraph);
const detachWire   = wireResolvedTree(graph); // canvas:tree (+ ui:* facts) → canvas:tree:resolved
const detachBridge = attachViewportBridge(graph); // window resize → ui:viewport (browser only)

// 2. Author a tree with responsive intent (intent stays on canvas:tree, pristine):
graph.put('canvas:tree', {
  id: 'root', type: 'Box', props: {},
  responsive: { direction: { base: 'column', md: 'row' }, gap: { base: '8px', md: '16px' } },
  children: [ /* ... */ ],
});

// 3. Unum reads `canvas:tree:resolved` — already sized for the current viewport.
//    On every resize the resolved tree is regenerated from the pristine source.
```

Server/test one-shot (no graph, no DOM):
```ts
const resolved = resolveUiTree(authoredTree, { viewport: { width: 1280 } });
```

**Defaults with zero intent:** a `container` with >1 child and no explicit
`responsive.direction` stacks to `column` below `md`, `row` at `md`+ — so layouts are
sensible even when the author declares nothing. Explicit `responsive.*` always wins.

## 10. The one integration handoff (separate surface)

Unum/the Tauri renderer must read **`canvas:tree:resolved`** instead of `canvas:tree`.
That is the only change outside this package; everything above is self-contained and
tested. Until that switch is made, the resolved tree is computed and stored but not yet
rendered — authored `canvas:tree` keeps working exactly as before (fully backward
compatible).

## 11. Follow-on (engine ready, practices not yet written — honestly absent)

- **theme / contrast** resolve practices (trigger `ui:theme`): the resolver + facts plumb
  `theme` through already; no `.px` practices authored yet.
- **density** resolve practices (trigger `ui:density`): same — plumbed, not authored.
- **`maxLines` truncation**: reserved in `RESPONSIVE_ATTRS`; no resolver default branch
  yet (responsive-map pass-through works, but there is no type-based default).
- These are *absent, not stubbed* (C-NOSTUB-001): no fake providers, no dead rules.
