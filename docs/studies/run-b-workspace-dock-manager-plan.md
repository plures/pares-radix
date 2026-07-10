# Run B Implementation Plan — WorkspaceLayout Dock Manager

**Status:** ANALYZE (no edits). Composes Run A primitives into a persisted, .px-resolved dock manager.
**Repo:** radix-wt-admin-console @ feat/admin-console-plugin

Ground truth confirmed by inspection:
- vitest include = `src/**/*.test.ts` → **pure dock logic must live in `src/lib/workspace/`** (root pkg), mirroring Run A's `src/lib/panes/`. design-dojo `.svelte` is the render boundary only.
- Persistence = `PraxisFact[]` (persist:true) registered in the adapter registry in `+layout.svelte`; write via `emitFact(id,value)`, read reactively via `query(id)`; `initPraxisFacts()`→`adapter.hydrateAll()` restores on reload. Same path as `admin.plugins.enabled`/`admin.feature.flags`.
- `.px` pattern: a `praxis/procedures/*.px` file + an **executable TS twin** `PraxisModule` (facts/events/constraints + pure decision helpers), exactly like `admin.ts` twins `admin-console.px`. Modules are wired in `+layout.svelte` (`registry`, `buildSchemaRegistry`, `registerForHotReload`).
- `MoveCommand` (from `src/lib/panes/dnd.ts`) is transport-only `{itemId,fromDock?,toDock,toIndex}`. Run B owns the dock state it applies against.
- design-dojo exports `EmptyState`, `WtSplitPane`, `WtPane`, `WtPaneTabs`. Lint: `plures/no-raw-html`, `plures/no-raw-stores` (use `emitFact`/`query`, not ad-hoc stores; escape-hatch comments allowed as in existing layout).

---

## 1. Ordered file list

**New — pure dock logic (root pkg, vitest-covered):**
1. `src/lib/workspace/types.ts` — `DockId`, `PaneInstance`, `DockState`, `WorkspaceLayoutState`, action union.
2. `src/lib/workspace/reducer.ts` — pure reducer `applyAction(state, action): WorkspaceLayoutState` + `applyMoveCommand(state, cmd)`.
3. `src/lib/workspace/reducer.test.ts` — unit tests for every action + MoveCommand.
4. `src/lib/workspace/persistence.ts` — pure `serializeLayout(state)` / `deserializeLayout(layoutFact, instanceFacts)` mapping state ⇄ the two fact shapes (no `emitFact` here — pure, testable).
5. `src/lib/workspace/persistence.test.ts` — round-trip serialize→deserialize equals identity; partial/hydration-missing tolerance.
6. `src/lib/workspace/dock-resolution.ts` — pure twins of the `.px`: `resolvePaneDock(preferred, override, allowedDocks)`, `resolveVisibility(defaultVisible, userHid)`.
7. `src/lib/workspace/dock-resolution.test.ts` — twin tests (mirrors `admin.test.ts` "twins of .px" convention).

**New — praxis module + .px (executable twin + source of truth):**
8. `praxis/procedures/workspace-layout.px` — `resolve_pane_dock` procedure + `pane_visibility` constraint (C-DEV-001 source of truth).
9. `src/lib/praxis/workspace.ts` — `workspaceModule: PraxisModule` (facts `workspace.layout`, `workspace.paneInstances.<id>`; constraint `pane_visibility`; helpers delegate to `dock-resolution.ts`).
10. `src/lib/praxis/workspace.test.ts` — module facts declared with correct persist flags; constraint holds/violates.

**New — reactive store bridge (Svelte layer, uses emitFact/query):**
11. `src/lib/stores/workspace-svelte.svelte.ts` — `useWorkspaceLayout()` projection + `dispatch(action)` that reduces then `emitFact`s the changed facts. Thin; no dock-decision logic.

**New — composition component (design-dojo render boundary):**
12. `packages/design-dojo/src/WorkspaceLayout.svelte` — composes WtSplitPane + WtPaneTabs per dock; wires pointer/keyboard DnD → dispatch; EmptyState for empty docks; center renders `{@render children()}`.
13. `packages/design-dojo/src/index.ts` — add `export { default as WorkspaceLayout } from './WorkspaceLayout.svelte';` + types.

**Edited:**
14. `src/routes/+layout.svelte` — wrap `WorkspaceLayout` around `PluginContentArea`'s children slot; register `workspaceModule` in adapter registry, `buildSchemaRegistry`, `registerForHotReload`; seed default layout (hydration-safe).

---

## 2. Dock-manager state model & reducer API

`src/lib/workspace/types.ts`:
```ts
export type DockId = 'center' | 'right' | 'bottom' | 'left';
export const DOCK_RING: DockId[] = ['center', 'right', 'bottom', 'left'];

/** Instance identity = `<pluginId>#<instanceId>`. */
export type InstanceId = string;

export interface PaneInstance {
  instanceId: InstanceId;   // e.g. "agens#1"
  pluginId: string;
  title: string;
  state?: Record<string, unknown>; // opaque pane-owned state (persisted)
}

export interface DockState {
  visible: boolean;
  size: number;                 // px extent of the dock along its split axis
  tabs: InstanceId[];           // ordered tab group (multiple instances allowed)
  activeTab: InstanceId | null;
}

export interface WorkspaceLayoutState {
  docks: Record<DockId, DockState>;
  instances: Record<InstanceId, PaneInstance>;
}

export type WorkspaceAction =
  | { type: 'moveInstance'; instanceId: InstanceId; toDock: DockId; toIndex: number }
  | { type: 'reorderInDock'; dock: DockId; from: number; to: number }
  | { type: 'setActive'; dock: DockId; instanceId: InstanceId }
  | { type: 'toggleDock'; dock: DockId; visible?: boolean } // omit = flip
  | { type: 'resizeDock'; dock: DockId; size: number }
  | { type: 'addInstance'; instance: PaneInstance; dock: DockId; index?: number }
  | { type: 'removeInstance'; instanceId: InstanceId };
```

`src/lib/workspace/reducer.ts` — **pure, immutable, framework-free**:
```ts
export function applyAction(s: WorkspaceLayoutState, a: WorkspaceAction): WorkspaceLayoutState;
export function applyMoveCommand(s: WorkspaceLayoutState, cmd: MoveCommand): WorkspaceLayoutState;
export function initialLayout(): WorkspaceLayoutState; // center visible, right/bottom/left hidden-or-empty, no instances
```
Reducer semantics (each returns a NEW state; never mutates):
- **moveInstance**: remove `instanceId` from its current dock's `tabs`; splice into `toDock.tabs` at clamped `toIndex` (−1 = append); update `instances[id].? ` via a `dockId` derivation is NOT stored on the instance (dock membership is authoritative via `tabs`); fix `activeTab` on both source dock (follow-neighbor via same logic as `tabs.closeTab`) and target dock (become active). No-op move (same dock+index) returns structurally-equal state.
- **applyMoveCommand**: delegates to `moveInstance` with `cmd.toDock`/`cmd.toIndex`; ignores `fromDock` (state is authoritative). This is the seam that consumes `dnd.ts` output.
- **reorderInDock**: reuse `panes/tabs.reorder` on the dock's `tabs`.
- **setActive**: set `activeTab` iff instance is in that dock.
- **toggleDock**: flip/set `visible`; center dock cannot be hidden (guard → returns state unchanged, keeps center always-on).
- **resizeDock**: set `size` (clamped ≥ 0; component clamps to min via `resize.clampSize`).
- **addInstance** / **removeInstance**: register/unregister in `instances` + dock `tabs`; remove reuses `tabs.closeTab` active-follow.

Reuse of Run A logic (no duplication, per anti-dup gate): `reorderInDock`→`panes/tabs.reorder`; active-follow on remove/move→`panes/tabs.closeTab`.

---

## 3. PluresDB fact schema (C-PLURES-003)

Registered in `src/lib/praxis/workspace.ts` as `PraxisFact[]` (same shape as `adminFacts`), added to the adapter `registry` in `+layout.svelte`:

```ts
const workspaceFacts: PraxisFact[] = [
  { id: 'workspace.layout', persist: true,
    description: 'Per-dock layout: Record<DockId,{visible,size,tabs:[instanceId],activeTab}>. ' +
      'Single source of truth for dock geometry/visibility/tab order; survives reload.' },
  // paneInstances are keyed per-instance so each instance persists independently.
  // Concrete keys: `workspace.paneInstances.<instanceId>` -> { pluginId, dockId, title, state }.
  { id: 'workspace.paneInstances', persist: true,
    description: 'Index of live instance ids: string[]. Per-instance detail stored under ' +
      'workspace.paneInstances.<instanceId> facts (also persist:true).' },
];
```

- `workspace.layout` value: `Record<DockId, DockState>`.
- `workspace.paneInstances` value: `InstanceId[]` (the index — lets hydration know which per-instance facts to read).
- `workspace.paneInstances.<instanceId>` value: `{ pluginId, dockId, title, state }` — **one fact per instance**, so a single pane's state can persist/rehydrate without rewriting the whole layout. `dockId` here is a denormalized convenience; `workspace.layout.<dock>.tabs` remains authoritative on conflict.

**Write path (Svelte bridge, `workspace-svelte.svelte.ts`):** every user drag/resize/collapse/reorder → `dispatch(action)` → `applyAction` → diff → `emitFact('workspace.layout', serializeLayout(next).layout)` and, for touched instances, `emitFact('workspace.paneInstances.'+id, {...})` + refresh the `workspace.paneInstances` index fact. `persist:true` makes the adapter write PluresDB immediately.

**Read/hydration path:** `initPraxisFacts()` already calls `adapter.hydrateAll()`. `useWorkspaceLayout()` = `$derived` projection: `deserializeLayout(query('workspace.layout'), instanceIds.map(id => query('workspace.paneInstances.'+id)))`. If `workspace.layout` is absent (first boot), seed `initialLayout()` (hydration-safe: seed only if `!query('workspace.layout')`, mirroring `wireAdminScene`/`wireOperationsScene`).

**persistence.ts** is pure: `serializeLayout(state) -> {layout, instanceIndex, instanceFacts}` and `deserializeLayout(layout, instanceFacts) -> WorkspaceLayoutState`. This keeps the mapping unit-tested away from Svelte.

---

## 4. `.px`-first dock resolution (C-DEV-001)

**Source of truth:** `praxis/procedures/workspace-layout.px` (sits beside `classify.px` etc.). Executable twin: `src/lib/workspace/dock-resolution.ts` + constraint in `workspace.ts` (same "twin of .px" convention as `admin.ts`/`admin.test.ts`).

`workspace-layout.px` sketch (following the repo's `procedure … given/check` + `# [parser-skip] constraint` style):
```
# workspace-layout.px — dock placement + visibility as pure logic (C-DEV-001).
# The Svelte layer only READS the resulting facts; no dock-decision logic in components.

# resolve_pane_dock: plugin preferredDock + optional user override -> actual dock.
procedure resolve_pane_dock(preferred: string, override: string, allowed: list) -> dock into "workspace.resolvedDock":
  given: "A pane lands in the user override dock if set & allowed, else its plugin preferredDock, else 'right'."
  select_dock { override: $override, preferred: $preferred, allowed: $allowed } -> $dock

# [parser-skip] constraint pane_visibility:
# [parser-skip]   given: "A defaultVisible pane is present in some dock unless the user explicitly hid it."
# [parser-skip]   check: FORALL p IN panes: (p.defaultVisible AND NOT userHidden[p.id]) IMPLIES present(p.id)
# [parser-skip]   severity: error
```

TS twins (`dock-resolution.ts`, pure):
```ts
export function resolvePaneDock(preferred: DockId, override: DockId | null, allowed: DockId[]): DockId;
  // override if allowed, else preferred if allowed, else 'right'
export function resolveVisibility(defaultVisible: boolean, userHid: boolean): boolean;
  // defaultVisible && !userHid
```
`workspace.ts` `pane_visibility` constraint (`PraxisConstraint`) checks the live `workspace.layout` + instance facts: any instance whose plugin declares `defaultVisible` and that the user did not explicitly hide MUST appear in some dock's `tabs`. Severity `error`. Components never decide docks — they call `dispatch` and render `query()` results.

---

## 5. WorkspaceLayout.svelte composition (C-NOSTUB-001)

`packages/design-dojo/src/WorkspaceLayout.svelte`:
- **Props:** `layout: WorkspaceLayoutState`, `children: Snippet` (the routed center page), `paneBody: Snippet<[PaneInstance]>` (host renders a docked instance's real surface — Run C provides agens; Run B passes a Snippet that renders EmptyState-per-instance honestly), `ondispatch: (a: WorkspaceAction) => void`.
- **Structure** (nested WtSplitPane, per study §4.4):
  - Outer `WtSplitPane orientation="vertical"`: `a` = center-stack, `b` = **bottom dock**; sash hidden/collapsed when `bottom.visible=false`.
  - center-stack = `WtSplitPane orientation="horizontal"`: `a` = **center** (`{@render children()}`), `b` = **right dock**; collapsed when `right.visible=false`.
  - (left dock optional in v1: a third horizontal split wrapping center on the leading side; ship the slot, hidden by default — honest, not fake.)
  - Each non-center dock body = `WtPane` (title/collapse) wrapping `WtPaneTabs` bound to that dock's `tabs`/`activeTab`, `panel={(id) => paneBody(instances[id])}`.
- **DnD wiring:** WtPaneTabs already handles intra-dock reorder → `ondispatch({type:'reorderInDock',...})`. Cross-dock: tab `pointerdown`→`beginDrag` (from `panes/dnd.ts`), `pointermove`→`updateDrag` with a `hitTest` that maps pointer to dock+index (dock elements carry `data-dock`), `pointerup`→`endDrag`→`MoveCommand`→`ondispatch(applyMoveCommand seam)` i.e. `{type:'moveInstance', instanceId, toDock, toIndex}`. Keyboard: on a focused tab, Arrow keys→`keyboardMove(item, key, visibleDockIds, curDock)`→same dispatch. a11y already provided by WtPaneTabs roving tabindex.
- **Resize/collapse:** WtSplitPane `onresize`→`ondispatch({type:'resizeDock',dock,size})`; `oncollapse`/WtPane header toggle→`ondispatch({type:'toggleDock',dock})`.
- **Empty docks:** when `dock.visible && dock.tabs.length===0`, render design-dojo `EmptyState` (real component, honest "No panes docked here" copy) — NEVER fabricated content (C-NOSTUB-001). Right/bottom start empty-but-real in Run B; the routed center is untouched.
- **Lint:** no raw `<script>`-injected HTML (`plures/no-raw-html`); all state via the passed-in `layout` prop + `ondispatch` (no ad-hoc store → satisfies `plures/no-raw-stores`; the store lives in `workspace-svelte.svelte.ts`).

---

## 6. `+layout.svelte` integration (routing unaffected)

Current: `<PluginContentArea …>{@render children()}</PluginContentArea>` (single slot).

Change:
1. In the `onMount` adapter block, add `...workspaceModule.facts` to the `registry` array; add `workspaceModule` to `buildSchemaRegistry(...)` and `registerForHotReload(workspaceModule)`.
2. After hydrate, seed default layout hydration-safely: `if (!query('workspace.layout')) emitFact('workspace.layout', serializeLayout(initialLayout()).layout)` (via a `wireWorkspaceScene(emitFact, query)` helper, matching `wireAdminScene`).
3. Reactive projection: `const workspace = useWorkspaceLayout();` (bridge) + `const dispatch = ...` from `workspace-svelte.svelte.ts`.
4. Wrap the center slot:
```svelte
<PluginContentArea …>
  <WorkspaceLayout layout={workspace} ondispatch={dispatch} {paneBody}>
    <Breadcrumbs />
    {@render children()}    <!-- center KEEPS the routed page: routing unaffected -->
  </WorkspaceLayout>
</PluginContentArea>
```
`{@render children()}` stays inside center → SvelteKit routing is byte-for-byte unchanged; docks surround it. `paneBody` in Run B is a Snippet rendering an honest per-instance EmptyState (Run C swaps in the real agens surface). Keep the existing `eslint-disable-next-line plures/no-raw-stores` convention on the new `$derived` bindings.

---

## 7. Unit tests (each real, no fixture-faking — C-TEST-002)

`reducer.test.ts`:
- moveInstance right→bottom updates both docks' `tabs` + `activeTab`; source active follows neighbor.
- moveInstance with `toIndex=-1` appends; with mid index splices at position.
- applyMoveCommand from a real `endDrag()` output lands the instance (integration with `dnd.ts`).
- reorderInDock matches `tabs.reorder` result.
- setActive only when instance present in dock; ignored otherwise.
- toggleDock flips; **center refuses to hide** (stays visible).
- resizeDock sets size; negative clamps to 0.
- addInstance/removeInstance register/unregister; remove active-follows.
- no-op move returns structurally-equal state.

`persistence.test.ts`:
- `deserializeLayout(serializeLayout(s))` === `s` (round-trip identity) for multi-dock, multi-instance state.
- missing `workspace.layout` fact → `initialLayout()` fallback (hydration tolerance).
- per-instance fact carries `{pluginId,dockId,title,state}`; state survives round-trip.

`dock-resolution.test.ts` (twins of `.px`):
- override (allowed) wins; override (not allowed) falls to preferred; neither allowed → 'right'.
- `resolveVisibility`: defaultVisible & !userHid → true; userHid → false.

`workspace.test.ts`:
- `workspace.layout` & `workspace.paneInstances` facts declared `persist:true`.
- `pane_visibility` constraint: violates when a defaultVisible, non-hidden instance is absent from all docks; holds otherwise.

(Component render logic is covered by the reducer/persistence purity + the existing WtSplitPane/WtPaneTabs Run A tests; the `.svelte` shell adds no new decision logic to test — it delegates to `ondispatch`.)

---

## 8. How VERIFY asserts persistence across reload

Verify stage (the C-gate — real run, not fixture):
1. Boot app (dev server / Tauri), navigate center to route A.
2. Programmatically add two instances to `right` (e.g. `agens#1`, `agens#2`) via `dispatch(addInstance)`.
3. Drag `agens#1` from `right`→`bottom` (pointer path through WorkspaceLayout, producing a real `MoveCommand`), then resize/collapse a dock.
4. Read `query('workspace.layout')` and assert `bottom.tabs` contains `agens#1`, `right.tabs` does not, and dock size/visible reflect the change.
5. **Reload the page** (or restart Tauri) → `initPraxisFacts()`→`hydrateAll()`.
6. Assert `query('workspace.layout')` after reload STILL shows `agens#1` in `bottom` with the same size/visibility, and `query('workspace.paneInstances.agens#1').dockId === 'bottom'` with its `state` intact.
7. Assert center still renders route A's routed page (routing unaffected) and empty docks show real `EmptyState`, not fabricated content.

This is the persistence proof: a dock→dock move survives a full reload because it is a `persist:true` fact, not component-local state.

---

## End Result: **PASS**

Analyze lane complete — no edits made. Plan delivers pure vitest-covered d