# Study: VS Code Pane Model → Radix Multi-Pane Workspace

**Date:** 2026-07-10
**Author:** mswork (for kbristol)
**Repo:** pares-radix / radix-wt-admin-console (branch feat/admin-console-plugin)
**Status:** Study + design proposal (pre-implementation)

## 1. Problem Statement

Radix today renders **exactly one plugin surface at a time**. The layout is:

```
+----------+--------------------------------------------+
| Sidebar  |  PluginContentArea                         |
| (nav)    |    topbar                                   |
|          |    <main>{@render children()}</main>  <-- ONE surface |
|          |    statusbar                                |
+----------+--------------------------------------------+
```

`PluginContentArea.svelte` (design-dojo) has a single `<main>` slot. `+layout.svelte`
routes one page into it. Choosing "Agens" replaces whatever was there; choosing
"Operations" replaces Agens. There is no way to keep an agent pane docked while
working in another plugin.

**Desired capability (kbristol):** certain plugins - agens especially - must be
**always visible and available to work alongside any other plugin**, exactly like
VS Code's Terminal and Debug Console panes stay docked while you edit code in the
main editor group.

## 2. How VS Code Actually Structures Panes

VS Code's workbench is a nested set of resizable regions, not a single content slot:

| VS Code region | Role | Radix analog (target) |
|---|---|---|
| **Activity Bar** (far left icons) | Switches the Primary Sidebar view | Radix `Sidebar` (nav) - already exists |
| **Primary Sidebar** | Explorer / Search / SCM etc. | (folds into Sidebar for now) |
| **Editor Group(s)** | The main work area; can be **split** (grid of groups), each with tabs | Radix "main pane" - the routed plugin surface |
| **Panel** (bottom, dockable) | Terminal, Debug Console, Problems, Output - **tabbed, persists across editor changes** | **The missing piece** - a docked secondary pane |
| **Secondary Sidebar** (right) | Optional extra dock (Copilot Chat lives here by default) | **The agens dock target** |
| **Status Bar** | Global status | Radix statusbar - already exists |

Key VS Code properties we must replicate:

1. **Panes are independent of the main editor.** Switching files/plugins in the
   center does NOT unmount the Terminal/Debug/Chat pane. Their lifecycle is
   orthogonal to the main surface's lifecycle.
2. **Panes are dockable + movable.** The Panel can be bottom/left/right; Copilot
   Chat can be in the Secondary Sidebar (right) or moved to the Panel. A plugin
   declares a *preferred dock*, the user can override it.
3. **Panes are resizable + collapsible.** Sashes (drag handles) between regions;
   each pane can be hidden/shown without losing its state.
4. **Panes can be tabbed.** The bottom Panel hosts multiple views (Terminal +
   Problems + Output) as tabs in one region.
5. **State survives.** A running terminal keeps running; chat history stays;
   scroll position persists - because the pane's component instance is never
   torn down when the center changes.

The one that matters most for agens: **#1 (orthogonal lifecycle) + #2 (secondary
sidebar dock)**. Copilot Chat in VS Code is precisely the pattern kbristol wants
for agens - a persistent agent pane you consult while working in any other surface.

## 3. Current Radix Inventory (ground truth)

- `packages/design-dojo/src/PluginContentArea.svelte` - single `<main>` slot. **No split.**
- `packages/design-dojo/src/index.ts` - exports Sidebar, PluginContentArea,
  DashboardGrid, PluginModule, etc. **No SplitPane / Pane / Dock / Panel / Tabs
  component exists in source.** (An earlier junction-confused `ls` suggested some
  existed; the real `src/` does not have them - they must be built.)
- `src/routes/+layout.svelte` - `<Sidebar/>` + `<PluginContentArea>{@render children()}</PluginContentArea>`. One routed surface.
- Plugin surfaces are **static SvelteKit routes** today; plugin-contributed
  routes are NOT dynamically mounted yet (documented follow-up: `[...plugin]` router).
- `RadixPlugin` now has `type: 'panel' | 'agent'` (shipped this session). This is
  the hook we extend: an `'agent'`-type (or dock-capable) plugin declares a dock.

## 4. Proposed Architecture: Radix Multi-Pane Workspace

### 4.1 New design-dojo primitives (the "pane implementation" kbristol asked for)

Build these in `packages/design-dojo/src/`, VS Code-grade, theme-token driven:

1. **`SplitPane.svelte`** - two-child resizable split with a draggable sash.
   Props: `orientation: 'horizontal' | 'vertical'`, `initialSize`, `minSize`,
   `collapsed`, persisted size via a bindable. The atomic building block.
2. **`Pane.svelte`** - a titled, collapsible dock region (header + body +
   optional actions). Wraps content that lives in a dock slot.
3. **`PaneTabs.svelte`** - tabbed container for multiple views in one dock
   (the VS Code bottom-Panel pattern: Terminal | Problems | Output).
4. **`WorkspaceLayout.svelte`** - the composition root that replaces the raw
   single-slot usage: named dock regions **center / right / bottom** (+ optional
   left), each a `SplitPane`/`Pane`, with the center hosting the routed surface
   and right/bottom hosting docked plugin panes. Docks collapsible + resizable +
   state-persisted. This is Radix's "workbench."

These are honest, real components - no stubs. `SplitPane` is the only one with
real interaction logic (sash drag + keyboard resize + a11y `separator` role);
the rest compose it.

### 4.2 Plugin dock contract (extends the type work from this session)

Extend `RadixPlugin` so a plugin can declare it renders as a **dockable pane**,
not just a nav-routed surface:

```ts
interface PaneContribution {
  id: string;
  title: string;
  icon?: string;
  preferredDock: 'right' | 'bottom' | 'left';   // like VS Code default location
  defaultVisible?: boolean;                       // agens: true
  singleton?: boolean;                            // one instance, persists
}
// RadixPlugin gains:  panes?: PaneContribution[];
```

- `agens` (type `'agent'`) contributes a pane: `{ preferredDock: 'right',
  defaultVisible: true, singleton: true }` - VS Code Copilot-Chat placement.
- The **lifecycle is orthogonal**: the agens pane component mounts ONCE in the
  right dock and stays mounted while the center route changes. This is the crux -
  it must NOT be a child of the routed page.

### 4.3 State model (PluresDB facts, per C-PLURES-003)

Layout is domain state → PluresDB facts, not local component state:

- `workspace.layout` → `{ right: {visible, size}, bottom: {visible, size, activeTab}, ... }`
- `workspace.panes.<dockId>` → ordered list of pane contributions mounted there
- User dock/resize/collapse actions `emitFact` these; `WorkspaceLayout` is a
  reactive `query()` projection. Survives reload, same pattern as `admin.plugins.*`.

### 4.4 .px-first (per C-DEV-001)

Dock placement + visibility rules are logic → express as a `.px` procedure:
`resolve_pane_dock` (plugin preferredDock + user override → actual dock),
`pane_visibility` constraint (a `defaultVisible singleton` agent pane is present
unless the user explicitly hid it). The Svelte components are the IO/render
boundary that read the resulting facts. No dock decision logic baked into
components.

## 5. Honesty / Boundary Notes (C-NOSTUB-001)

- The agens pane in the **browser** shows the same honest "Agent runtime
  unavailable - desktop only" state we just shipped - but now docked in the right
  pane instead of a full route. No fake chat.
- `SplitPane` sash must be real drag + keyboard, not a decorative divider.
- Persisted layout must actually restore on reload (verify stage will assert it),
  not just visually appear.

## 6. Proposed Delivery (separate dev-lifecycle runs)

This is too big for one lifecycle. Staged as three:

- **Run A - design-dojo pane primitives:** build + unit-test `SplitPane`, `Pane`,
  `PaneTabs`, `WorkspaceLayout`. Storybook/showcase in the `/design` dojo page.
  Ships the reusable "pane implementation" independent of Radix wiring.
- **Run B - Radix workspace shell:** replace `PluginContentArea` single-slot usage
  in `+layout.svelte` with `WorkspaceLayout`; wire `workspace.layout` PluresDB
  facts; center = routed surface, docks empty-but-functional.
- **Run C - agens as a docked pane:** add `panes` to `RadixPlugin` + the
  `resolve_pane_dock` .px; agens contributes a right-dock singleton pane, mounted
  orthogonally so it persists across center navigation. Verify: navigate center
  A→B→C, assert agens pane stays mounted + retains state.

Each run: analyze → fix → test → deploy → verify, same gates as this session.

## 7. Decisions (RESOLVED by kbristol 2026-07-10)

1. **Default dock for agens:** RIGHT (Copilot-style secondary sidebar). Confirmed.
2. **v1 scope:** FULL - movable (drag-to-move panes between docks) AND tabbed dock
   groups are in v1, not a follow-up. `PaneTabs` + a real dock manager are first-class.
3. **Instances:** panes support MULTIPLE INSTANCES (e.g. two terminals). No singleton
   constraint; each pane instance owns its own state. `singleton` becomes an optional
   hint, not an enforced cap.

### Impact on the build

- `PaneContribution` drops the hard singleton requirement: `allowMultiple?: boolean`
  (default true). Agens sets `preferredDock: 'right', defaultVisible: true`.
- `WorkspaceLayout` needs a real **dock manager**: named docks (center/right/bottom/left),
  each dock is a **tab group** of pane instances; panes drag between docks and reorder
  within a dock. Instance identity = `<pluginId>#<instanceId>`.
- State model expands: `workspace.layout` holds per-dock `{ visible, size, tabs: [instanceId], activeTab }`
  and `workspace.paneInstances.<instanceId>` -> `{ pluginId, dockId, title, state }`.
  Still PluresDB facts (C-PLURES-003); still `.px`-resolved dock placement (C-DEV-001).
- Drag-to-move + tab reordering are real DnD (pointer events + keyboard move commands
  for a11y), not decorative - honesty gate (C-NOSTUB-001) applies.
