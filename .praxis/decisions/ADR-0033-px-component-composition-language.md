# ADR-0033: `.px` as a Component *Composition* Language (View-Trees over design-dojo Primitives)

- **Status:** Accepted (DESIGN stage; DEV proof-slice follows)
- **Date:** 2026-07-12
- **Deciders:** kbristol (strategic directive), dev-lead orchestrator
- **Relates:** ADR-0032 (GraphView primitive), ADR-0024 (canonical plugin format / design-dojo UI home),
  ADR-0020 (single PluresDB reactive memory), ADR-0031 (agens drives host-mediated navigation)
- **Invariants:** C-PLURES-003/004 (state & logic in PluresDB), C-DRIFT-001 (no manual sync steps),
  C-NOSTUB-001, C-TEST-001/002

---

## 1. Context

Today there is a hard wall between the two halves of the system:

- **`.px`** describes *logic / data / constraints* → compiles to PluresDB procedures. The brain.
- **`@plures/design-dojo`** describes *pixels* → Svelte + CSS. The face. It consumes **zero `.px`**
  (verified 2026-07-12: the only `.px` references in the package are code comments).

The only bridge between them is a **schema**: a plugin declares an entity's fields and DataGrid /
SchemaForm render *that one entity*. **Everything else about a surface** — which primitives appear,
how they are arranged, what data each is bound to, what actions they invoke — is hand-authored TS +
Svelte, in a separate artifact from the `.px` logic it visualizes.

That separation is the drift generator C-DRIFT-001 exists to kill: a feature is `.px` (logic) **plus**
Svelte (UI) **plus** wiring, three artifacts kept in sync by hand. It also blocks the natural
extension of ADR-0031: agens can drive *navigation* (emit a focus-change), but it **cannot compose a
surface**, because a surface is code, not data.

GraphView (ADR-0032) is the proof that the *right seam already exists inside a primitive*:
`graph-layout.ts` is a pure `(neighborhood, container box) → placements` function (Clay-style,
25/25 tests green, verified 2026-07-12) and `GraphView.svelte` only renders placements. The primitive
already cleanly separates pure logic from rendering. What is missing is the layer *above* the
primitive: a way to say, **as PluresDB data**, "put a GraphView here, bound to this focus, with these
actions" — without writing a bespoke Svelte page.

## 2. Decision

Introduce **`.px` as a component *composition* language**: a `.px` file can declare a **view-tree** —
a nested arrangement of **design-dojo primitives** bound to PluresDB data and actions. The view-tree
compiles to PluresDB nodes (like every other `.px` artifact); a thin **renderer** in the host
interprets the view-tree and mounts the corresponding design-dojo primitives.

This is deliberately scoped to **composition and binding**, NOT to layout math and NOT to CSS.

### 2.1 Scope — the three layers, and which one `.px` owns

| Layer | Owner | Rationale |
|---|---|---|
| **Composition / binding** — *which* primitives, *where* in the tree, bound to *what* PluresDB data, invoking *what* actions | **`.px` (NEW — this ADR)** | Pure data; drift-proof; agens-authorable; one source of truth |
| **Internal layout math** — a primitive's own allocation (e.g. GraphView's radial `graph-layout.ts`) | **TS (unchanged)** | Already pure + tested; `.px` here would add indirection with no strategic gain (ADR-0032 §2.2 decision) |
| **Rendering / pixels** — turning a placement into DOM/CSS/TUI | **Svelte + CSS (unchanged)** | Browser-native perf, a11y, transitions; reimplementing in `.px` is the "CSS-in-`.px`" tar pit we reject |

The tie-breaker gate (kbristol, 2026-07-12): **`.px` earns a layer only if it makes design easier,
less error-prone, more performant, or agens-drivable.** Composition passes on all four (one artifact,
no hand-sync, agens can emit a view). Layout-math and CSS fail the gate → they stay put.

### 2.2 View-tree data contract (initial)

```ts
// A view-tree node: a design-dojo primitive + its data binding + its actions.
interface ViewNode {
  primitive: string;                 // 'GraphView' | 'DataGrid' | 'SchemaForm' | 'DashboardGrid' | ...
                                     // the FIXED VOCABULARY = design-dojo's exported primitives
  bind?: ViewBinding;                // where this primitive reads its data in PluresDB
  props?: Record<string, unknown>;   // static props passed through to the primitive
  actions?: ViewAction[];            // action id -> a `.px` procedure to run on invoke
  children?: ViewNode[];             // composition (e.g. a DashboardGrid of panels)
}

interface ViewBinding {
  query: string;                     // a PluresDB query/selector the host resolves reactively
  as?: string;                       // prop name the resolved data is passed as (default per-primitive)
}

interface ViewAction {
  id: string;                        // matches the primitive's action id (e.g. GraphNodeAction.id)
  procedure: string;                 // `.px` procedure invoked on the PluresDB side (auditable)
}
```

- **The vocabulary is closed and versioned:** a view-tree may only reference primitives design-dojo
  actually exports. An unknown `primitive` is a compile-time error, not a runtime stub (C-NOSTUB-001).
- **Binding is reactive over PluresDB** (ADR-0020): the host resolves `bind.query`, passes the result
  to the primitive, and re-resolves on change. The primitive stays a pure view of data it is given
  (C-PLURES-003) — GraphView's existing `onFocusChange → host re-queries` contract is exactly this.
- **Actions are `.px` procedures**, so invoking a node action is a PluresDB write that triggers
  auditable logic — not a UI callback with hidden side effects. This is what lets **agens compose and
  drive a surface** (ADR-0031 generalized from navigation to composition).

### 2.3 Renderer (the thin interpreter)

A single host-side renderer walks a `ViewNode` tree and mounts design-dojo primitives:
- resolve `bind.query` → data (reactive subscription in PluresDB),
- pass `props` + bound data into the primitive,
- map primitive callbacks (`onFocusChange`, `onAction`, …) to `bind`/`action.procedure` writes.

The renderer is a **thin translation layer** (C-TEST-002): it holds no view state of its own; the
view-tree and its data live in PluresDB. GUI mounts Svelte primitives; TUI mounts their TUI token
sets — same view-tree, per ADR-0032's dual-token precedent.

## 3. What this is NOT (guardrails against re-litigating the wrong fight)

- **Not** a replacement for CSS or a primitive's internal layout math (§2.1). GraphView's
  `graph-layout.ts` stays TS; its `.svelte`/CSS rendering stays as-is.
- **Not** an open-ended UI DSL where arbitrary markup/interactions are expressed in `.px`. Rich
  interactions (drag, animation curves, focus management) live *inside* primitives in TS; the
  view-tree only *composes and binds* them. If a surface needs an interaction no primitive offers,
  the answer is a new/extended **primitive**, not an escape hatch in `.px`.
- **Not** a new state store. All view-tree + bound data is PluresDB (C-PLURES-003/004).

## 4. Consequences

**Positive**
- **One source of truth per surface.** Logic + view are the same `.px`-derived PluresDB data; the UI
  is a *derivation* of logic, not a parallel artifact — C-DRIFT-001 satisfied by construction.
- **agens can author and drive UI**, not just navigate it: emitting a view-tree is a PluresDB write,
  the natural generalization of ADR-0031.
- **design-dojo becomes an interpreter of a fixed primitive vocabulary**, not N bespoke pages — the
  same "one declaration → many backends" insight as Clay, with `.px` as the declaration.
- **Future primitives compose for free**: once a primitive is in the vocabulary, every view-tree can
  use it with no new wiring.

**Costs / risks**
- **A new renderer/interpreter layer** = real surface area and a "everything becomes a framework"
  risk. Mitigation: keep the vocabulary *closed* (only design-dojo exports), keep the renderer
  *stateless*, and grow the view-node contract only when a concrete surface needs it — no speculative
  generality.
- **Binding-query expressiveness** must match real surfaces without becoming a second query language.
  Mitigation: reuse the existing PluresDB query/selector surface; do not invent a parallel one.
- **Boundary leakage** (composition creeping toward layout/interaction). Mitigation: §3 guardrails +
  an expectation that fails a build if a view-tree tries to express pixel layout.

## 5. Acceptance criteria (verify stage — build the binary)

- A `.px` view-tree referencing `GraphView` bound to a PluresDB graph neighborhood **renders the live
  GraphView primitive** with real data (closes ADR-0032's open "no host drives it" gap).
- Selecting a stub re-queries the neighborhood via the `bind` contract and re-centers — proving
  reactive host-mediated navigation flows through the view-tree, not bespoke code.
- An unknown `primitive` in a view-tree is a **compile/validate error**, never a runtime stub.
- A node `action` invokes its `.px` procedure (auditable PluresDB write), verified without any channel
  adapter (C-TEST-002) — via the core API, not Telegram/Discord.
- The **same view-tree** renders through the TUI token path.
- agens emits a valid view-tree as a tool call and the renderer mounts it (proves agens-authored UI).
- `svelte-check` + view-tree validator clean; no stubs (C-NOSTUB-001).

## 6. References

ADR-0032 (GraphView primitive; the proof that pure-logic/render separation already exists),
ADR-0024 §5 (design-dojo as UI home / vocabulary owner), ADR-0020 (single PluresDB reactive memory),
ADR-0031 (agens host-mediated navigation → generalized here to composition);
C-PLURES-003/004, C-DRIFT-001, C-NOSTUB-001, C-TEST-001/002.
