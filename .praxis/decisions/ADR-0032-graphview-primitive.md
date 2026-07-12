# ADR-0032: GraphView — Ego-Centric, Space-Adaptive Graph Navigation Primitive

- **Status:** Accepted (DESIGN stage; DEV/verify stages follow)
- **Date:** 2026-07-11
- **Deciders:** kbristol (strategic directive), dev-lead orchestrator
- **Relates:** ADR-0024 (canonical plugin format / design-dojo UI home §5), ADR-0020 (single PluresDB reactive memory), ROADMAP Phase B
- **Invariants:** C-PLURES-003/004 (state/logic in PluresDB), C-NOSTUB-001, C-TEST-001/002

---

## 1. Context

`DataGrid` (Phase B) renders a *collection* — rows of one entity type. But PluresDB is a **graph**:
nodes with typed edges to other nodes. A tabular grid cannot express "this object, its
relationships, and where they lead." We need a first-class peer to `DataGrid` that renders graph
data with graph semantics: a focused node, its edges, and the linked neighbors — with drill-down,
graph-walking, and progressive disclosure driven by available space.

kbristol's specification (the design target):
- Display a focused object **in the center**, surrounded by its edges, each edge terminating in a
  **stub** of a linked object.
- **Selecting a stub re-centers it** — it becomes the focus, now surrounded by *its* edges/stubs.
  This is graph-walking by navigation, not by loading a static diagram.
- **How much is shown is governed by available space**, prioritizing the center/selected object.
  The layout behaves like flexbox: it **resizes and reflows to optimize for the space it's given**,
  so the *same control* looks right on a phone and across multiple large desktop screens with **no
  special per-breakpoint treatment**.
- Where space allows: **zoom**, and **expand individual objects** (a stub grows into a fuller card)
  **without disturbing other objects unless their minimums are violated** — i.e. neighbors only
  reflow/collapse when the expansion actually steals space they need.
- **Auto-summarization**: when a node/stub can't get the room for full detail, it degrades
  gracefully to a summary (title → title+key-fields → full card) rather than clipping.
- **Expansion buttons / actions**: per-node affordances to drill down, walk the graph, or invoke
  node actions.

## 2. Decision

Add **`GraphView`** to `@plures/design-dojo` as a schema- and graph-driven primitive, peer to
`DataGrid`. It is a *view* over graph data the plugin supplies; all graph state lives in PluresDB
(C-PLURES-003) — `GraphView` never owns persistent state, it renders a focus + neighborhood the
host reads from PluresDB and re-queries on navigation.

### 2.1 Data contract (graph semantics)
```ts
interface GraphNode {
  id: string;
  type?: string;                 // entity type -> drives which schema/summary applies
  label: string;
  fields?: Record<string, unknown>; // for progressive detail levels
  actions?: GraphNodeAction[];   // drill-down / custom actions
}
interface GraphEdge {
  id: string;
  from: string; to: string;
  label?: string;                // relationship name
  directed?: boolean;
}
interface GraphNeighborhood {    // what the host provides for the current focus
  focusId: string;
  nodes: GraphNode[];            // focus + neighbor stubs
  edges: GraphEdge[];
}
interface GraphViewProps {
  neighborhood: GraphNeighborhood;
  onFocusChange?: (nodeId: string) => void;  // re-center -> host re-queries PluresDB
  onExpand?: (nodeId: string) => void;       // request fuller detail (host may load more fields)
  onAction?: (nodeId: string, actionId: string) => void;
  detailFor?: (node: GraphNode, space: SpaceBudget) => DetailLevel; // optional override of the auto-summarizer
  minNodeSize?: { w: number; h: number };    // the "minimum" that governs neighbor reflow
  class?: string;
}
```
Navigation is **host-mediated**: selecting a stub fires `onFocusChange`, the host re-queries the
PluresDB neighborhood of the new focus and passes a fresh `neighborhood`. `GraphView` does not walk
the graph itself — it renders the neighborhood it is given. This keeps graph traversal auditable
and in PluresDB, not in component state.

### 2.2 Layout model — "graph-flex" (the space-adaptive core)
A constraint/space-budget layout, not a physics force-graph:
1. **Center allocation first.** The focus node gets priority: it is placed centrally and granted
   the largest detail level its content + space allow.
2. **Radial edge/stub placement.** Edges emanate from the focus; each stub is placed along its edge
   in the remaining space, distributed to minimize overlap (angular distribution weighted by edge
   count and container aspect ratio — a wide multi-monitor container spreads horizontally, a phone
   stacks more vertically). **This is the flexbox analogue:** one control, container-driven, no
   breakpoint code.
3. **Space budget → detail level (auto-summarization).** Each node computes a `DetailLevel`
   (`icon` → `title` → `title+keyFields` → `full`) from the box it can be granted. Shrinking the
   container demotes outer stubs first (center is protected), so the control degrades from a rich
   desktop layout to a legible phone layout continuously.
4. **Independent expansion with minimum-aware reflow.** `onExpand`/zoom grows one node's target
   box. Neighbors keep their positions/sizes **unless the expansion forces a neighbor below
   `minNodeSize`** — only then do the affected neighbors reflow/demote (the "unless their minimums
   are affected" rule). Unaffected nodes are untouched (no global relayout jitter).
5. **Zoom** is a container-space multiplier that feeds back into the space budget (zooming in grants
   more px → higher detail levels / room to expand).

Implementation: CSS-driven sizing (container queries + `ResizeObserver` for the space budget) with
absolute radial positioning for edges/stubs; **GUI + TUI token sets** like every design-dojo
primitive (TUI degrades to a focus card + a list of labeled edges — same data contract, terminal
rendering). No physics engine dependency; no raw inline styles beyond scoped `:global`.

### 2.3 What GraphView is NOT
- Not a whiteboard/diagram editor (that's a different, later surface).
- Not a global force-directed "show me the whole graph" — it is **ego-centric** (one focus + its
  immediate neighborhood), which is what makes it bounded, fast, and mobile-viable.
- Not a state owner — no persistent state in the component (C-PLURES-003).

## 3. Consequences

**Positive** — a genuine graph UI for a graph database; one responsive control desktop→mobile with
no breakpoint forks; ego-centric bounding keeps it performant and legible; because navigation is
host-mediated over PluresDB, **agens can drive it too** (emit a focus-change / walk the graph as a
tool call — ADR-0031), and graph traversal is auditable. Directly serves the extensible-inventory
story (walk item → location → supplier → related items) and any relational plugin.

**Costs/risks** — the space-budget/radial layout is the hard part (mitigation: start with a proven
allocation pass — center-first, angular distribution, minimum-aware reflow — and cover it with
deterministic layout unit tests at fixed container sizes: phone-portrait, tablet, wide-desktop,
multi-monitor). Overlap avoidance for high-degree nodes needs a cap + "N more" summarization stub
(auto-summarization applies to *edge count* too, not just node detail).

## 4. Acceptance criteria (verify stage — build the binary)
- `GraphView` renders a focus + its edge-linked stubs from a `GraphNeighborhood`.
- Selecting a stub fires `onFocusChange`; re-passing a new neighborhood re-centers correctly.
- At three fixed container sizes (phone-portrait, tablet, wide-desktop) the layout produces
  legible, non-overlapping output with center prioritized — asserted by layout unit tests.
- Expanding one node does **not** move/resize neighbors that stay above `minNodeSize`; only
  minimum-violating neighbors reflow.
- High-degree focus collapses excess edges into an "N more" summarization affordance.
- TUI token path renders the same data contract as a focus card + edge list.
- `svelte-check` clean; no stubs (C-NOSTUB-001).

## 5. References
ADR-0020 (single PluresDB reactive memory), ADR-0024 §5 (design-dojo UI home), ADR-0031
(agens can drive host-mediated navigation), ROADMAP Phase B; C-PLURES-003/004, C-NOSTUB-001,
C-TEST-001/002.
