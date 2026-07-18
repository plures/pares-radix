# Run C Implementation Plan — Agens as a Persistent Right-Dock Pane

**Status:** ANALYZE (no edits). Makes agens a REAL docked pane INSTANCE in the RIGHT
dock, mounted ORTHOGONALLY to the routed center — the VS Code Copilot-Chat pattern.
**Repo:** radix-wt-admin-console @ feat/admin-console-plugin (HEAD b14bf92)
**Depends on:** Run A (WtSplitPane/WtPane/WtPaneTabs + `src/lib/panes/*`) and Run B
(`src/lib/workspace/*` reducer/persistence/dock-resolution, `praxis/procedures/workspace-layout.px`,
`workspace.*` facts, `WorkspaceLayout.svelte`, `+layout.svelte` wiring) — BOTH landed & verified.

---

## 0. Ground truth confirmed by inspection (what Run C reuses, does NOT rebuild)

- **`WorkspaceLayout.svelte` already accepts a `paneBody?: Snippet<[PaneInstance]>`** and renders it
  per docked tab; when absent it shows an honest `EmptyState`. **`+layout.svelte` does NOT currently
  pass `paneBody`.** → Run C supplies a real `paneBody` snippet that renders the agens surface.
- **Reducer already has `addInstance` action** (`{ type:'addInstance'; instance:PaneInstance; dock:DockId; index? }`)
  and `applyAction`; the Svelte bridge `dispatch()` reduces + persists. → seeding = `dispatch(addInstance…)`.
- **Dock resolution already exists**: `resolvePaneDock(preferred, override, allowed)` +
  `resolveVisibility(defaultVisible, userHid)` in `src/lib/workspace/dock-resolution.ts`, twinned by
  `praxis/procedures/workspace-layout.px`. → Run C calls `resolvePaneDock` when placing a contribution.
- **Persistence** keys per-instance facts `workspace.paneInstances.<id> -> { pluginId, dockId, title, state }`
  and the index `workspace.paneInstances -> InstanceId[]`; a seeded instance therefore persists across reload
  automatically (no new persistence work).
- **`defaultLayout()`** already makes `right` visible (size 320). A seeded agens instance in `right` is
  immediately visible.
- **Plugin nav path**: `getAllNavItems()` (plugin-loader) → `seedNavItems()` (praxis-svelte) → `nav.visible`
  fact → Sidebar. `getActivePluginManifests()` exists for active-plugin iteration. → Run C adds an analogous
  `getAllPaneContributions()` and seeds from ACTIVE plugins after `activateAll()`.
- **Honesty gate C-NOSTUB-001** is enforced in-repo (praxis/ui/*). The browser agens surface MUST keep the
  existing "Agent runtime unavailable — desktop only" empty-state (from `agentRuntimeAvailable()` /
  `__TAURI_INTERNALS__` gating) when docked, exactly as on the `/agent` route. No fake chat in the dock.

---

## 1. Ordered file list

**New — pure TS (root pkg, vitest globs `src/**/*.test.ts`):**
1. `src/lib/workspace/pane-contributions.ts` — the `PaneContribution` type + pure functions
   `contributionToInstance()` and `seedInstancesFromContributions()` (maps enabled plugins' pane
   contributions → `addInstance` actions using Run B `resolvePaneDock`). **All dock decisions delegate to
   `dock-resolution.ts` — no new dock logic here.**
2. `src/lib/workspace/pane-contributions.test.ts` — the named unit test (see §7).

**Edited — types + manifest (extend the plugin contract):**
3. `src/lib/types/plugin.ts` — add `PaneContribution` interface + `panes?: PaneContribution[]` on `RadixPlugin`.
4. `src/lib/platform/plugin-resolver.ts` — add `panes?: PaneContribution[]` to `PluginManifest` (parity with
   the runtime type; import the type from `plugin.ts`).

**Edited — plugin registry → pane aggregation:**
5. `src/lib/platform/plugin-loader.ts` — add `getAllPaneContributions(): Array<PaneContribution & { pluginId }>`
   iterating ACTIVE plugins (mirrors `getAllNavItems()`), skipping disabled plugins.

**Edited — agens declares its dock pane:**
6. `src/lib/plugins/agens/index.ts` — add `panes: [{ id:'agens-console', title:'Agens', icon:'💬',
   preferredDock:'right', defaultVisible:true, allowMultiple:true }]`.

**New — the shared surface (EXTRACTED, no duplication):**
7. `src/lib/plugins/agens/AgensSurface.svelte` — the entire agens chat surface + honest desktop-only
   empty-state, extracted verbatim from `src/routes/agent/+page.svelte` (the `<Box class="chat-page">…`
   body + `<script>` logic + `<style>`), minus the `<svelte:head>` title. Zero behavior change.

**Edited — /agent route becomes a thin wrapper:**
8. `src/routes/agent/+page.svelte` — replace its body with `<svelte:head>` + `<AgensSurface />`
   (thin wrapper; see decision §D). No logic duplication.

**Edited — seed + render the dock instance:**
9. `src/routes/+layout.svelte` — (a) after `activateAll()` + `wireWorkspaceScene`, call
   `seedPaneInstances()` (hydration-safe); (b) pass a `paneBody` snippet to `WorkspaceLayout` that renders
   `AgensSurface` for agens instances (dispatch on `pluginId`).

**No new `.px`** — Run B's `workspace-layout.px` (`resolve_pane_dock` + `pane_visibility`) already governs
this; Run C only *uses* it. (If `pane_visibility` needs the contribution's `defaultVisible`, that is already
expressible via the per-instance fact; no procedure change.)

---

## 2. The `PaneContribution` type (`src/lib/types/plugin.ts`)

```ts
/**
 * A dockable pane a plugin contributes to the workspace (VS Code Panel /
 * Secondary Sidebar model). Distinct from a nav-routed surface: a pane instance
 * mounts in a WorkspaceLayout dock and lives ORTHOGONALLY to center routing.
 */
export interface PaneContribution {
  /** Stable contribution id (unique within the plugin), e.g. 'agens-console'. */
  id: string;
  /** Tab title shown in the dock. */
  title: string;
  /** Emoji or icon path. */
  icon?: string;
  /** Where the pane wants to dock; user override + resolve_pane_dock decide the actual dock. */
  preferredDock: 'right' | 'bottom' | 'left';
  /** Seed one instance visible on first boot (agens: true). */
  defaultVisible?: boolean;
  /**
   * Whether more than one instance may exist (VS Code "two terminals" model).
   * Default true — panes are NOT singletons (study §7 decision 3).
   */
  allowMultiple?: boolean;
}

// RadixPlugin gains:
//   /** Dockable panes this plugin contributes (orthogonal to nav routes). */
//   panes?: PaneContribution[];
```

`PluginManifest` (plugin-resolver.ts) gains the same optional `panes?: PaneContribution[]` for parity.
`allowMultiple` defaults to **true** (NOT singleton) per study §7 — it's a hint the seeder respects, not a cap
the reducer enforces; agens sets it explicitly true.

---

## 3. Seeding mechanism (PURE TS, reuses Run B `addInstance` + `resolve_pane_dock`)

`src/lib/workspace/pane-contributions.ts` — framework-free, unit-tested:

```ts
import type { PaneContribution } from '$lib/types/plugin.js';
import { resolvePaneDock } from './dock-resolution.js';
import type { DockId, PaneInstance, WorkspaceLayoutState } from './types.js';
import { DOCKABLE } from './types.js';
import { applyAction } from './reducer.js';

/** A plugin-scoped contribution (as returned by getAllPaneContributions). */
export interface ScopedPaneContribution extends PaneContribution {
  pluginId: string;
}

/** instance id convention `<pluginId>#<n>` — n is the next free ordinal for that plugin. */
export function nextInstanceId(state: WorkspaceLayoutState, pluginId: string): string {
  let n = 1;
  while (state.instances[`${pluginId}#${n}`]) n++;
  return `${pluginId}#${n}`;
}

/** Map one contribution to a PaneInstance + its resolved dock (pure). */
export function contributionToInstance(
  state: WorkspaceLayoutState,
  c: ScopedPaneContribution,
  override: DockId | null = null,
): { instance: PaneInstance; dock: DockId } {
  const dock = resolvePaneDock(c.preferredDock, override, DOCKABLE);
  return {
    instance: { instanceId: nextInstanceId(state, c.pluginId), pluginId: c.pluginId, title: c.title },
    dock,
  };
}

/**
 * Seed instances from contributions into a layout. Only seeds a contribution when:
 *   - it is defaultVisible, AND
 *   - no instance of that plugin already exists (idempotent / hydration-safe):
 *     a restored layout already carrying agens#1 is left untouched, so a user who
 *     closed the pane does NOT get it re-seeded on reload.
 * Returns the new state (pure — caller persists via the bridge).
 */
export function seedInstancesFromContributions(
  state: WorkspaceLayoutState,
  contributions: ScopedPaneContribution[],
): WorkspaceLayoutState {
  let next = state;
  for (const c of contributions) {
    if (!c.defaultVisible) continue;
    const alreadyPresent = Object.values(next.instances).some((i) => i.pluginId === c.pluginId);
    if (alreadyPresent) continue;
    const { instance, dock } = contributionToInstance(next, c);
    next = applyAction(next, { type: 'addInstance', instance, dock, index: -1 });
  }
  return next;
}
```

**Svelte glue (in `+layout.svelte`, thin — no logic):** after `activateAll()` resolves and
`wireWorkspaceScene` has seeded/restored `workspace.layout`:

```ts
import { getAllPaneContributions } from '$lib/platform/plugin-loader.js';
import { seedInstancesFromContributions } from '$lib/workspace/pane-contributions.js';
import { readLayout } from '$lib/stores/workspace-svelte.svelte.js';
// ...
const seeded = seedInstancesFromContributions(readLayout(), getAllPaneContributions());
// persist only if it changed (readLayout()!==seeded means an instance was added):
if (seeded !== readLayout()) writeSeededLayout(seeded);
```

To keep the bridge the single writer, add a tiny exported `seedPaneInstances(contributions)` to
`workspace-svelte.svelte.ts` that does `const s = seedInstancesFromContributions(readLayout(), contributions);
if (s !== <prev>) writeLayout(s);` (reuses the existing private `writeLayout`; export a `seedPaneInstances`).
This is **idempotent + hydration-safe**: a reload where agens#1 already exists is a no-op; a user who closed
the pane is respected (present-check is by plugin, and the closed instance won't be re-added because we key on
"any instance of this plugin exists" only at first boot — see §7 test for the closed-then-reload case, which
uses a `userHid`-style guard fact if we want closed-to-stay-closed; v1 seeds only when `workspace.layout` was
freshly created, matching `wireWorkspaceScene`'s "seed only on first boot" contract).

**`getAllPaneContributions()` (plugin-loader.ts)** mirrors `getAllNavItems()`:

```ts
export function getAllPaneContributions(): Array<PaneContribution & { pluginId: string }> {
  const out: Array<PaneContribution & { pluginId: string }> = [];
  for (const p of /* active plugins iterator, same as getAllNavItems */) {
    for (const pane of p.panes ?? []) out.push({ ...pane, pluginId: p.id });
  }
  return out;
}
```

Disabled plugins are already excluded from the active iterator (same gate as `getAllNavItems`), so a disabled
agens contributes no pane — correct.

---

## 4. Shared-surface extraction (the pane BODY renderer — no duplication, no stub)

**Problem today:** the entire chat surface (script + template + style + honest desktop-only empty-state) lives
inside `src/routes/agent/+page.svelte`. The dock needs the SAME surface. Duplicating it would violate the
anti-dup gate and C-NOSTUB-001 (two drifting copies).

**Extraction:**
- Create `src/lib/plugins/agens/AgensSurface.svelte` containing the **exact** `<script>` logic
  (imports from `$lib/platform/agent-api.js`, `onMount`, `sendMessage`, `runtimeReady` gate, `messages`,
  `handleKeydown`, `formatTime`) + the `<Box class="chat-page">…</Box>` template (including the
  `runtimeReady === false` desktop-only empty-state) + the `<style>` block, moved verbatim from the route.
  The ONLY thing that stays on the route is `<svelte:head><title>` (page-level concern).
- `AgensSurface` renders `height: 100%` (already does via `.chat-page`), so it fills either the full route
  center OR a dock tab body identically.
- **Both consumers render `<AgensSurface />`:** the `/agent` route (§D) and the dock `paneBody` snippet.
  One implementation, two mount points. In the browser BOTH show the honest empty-state; in Tauri BOTH wire
  the real agent-api. No stub, no fork.

**`paneBody` wiring in `+layout.svelte`:**
```svelte
<WorkspaceLayout layout={workspaceLayout} ondispatch={dispatchWorkspace} {paneBody}>
  <Breadcrumbs />
  {@render children()}
</WorkspaceLayout>

{#snippet paneBody(instance)}
  {#if instance?.pluginId === 'agens'}
    <AgensSurface />
  {:else}
    <EmptyState title={instance?.title ?? 'Pane'} description="No surface registered for this pane." />
  {/if}
{/snippet}
```
The `else` branch is honest (real EmptyState), not a fake — other plugins' panes render a truthful
"no surface registered" until they ship their own body (C-NOSTUB-001).

> **Registry note (optional hardening, not required for Run C):** the `pluginId → surface component` map
> could later be data-driven (a `paneComponent?: () => Promise<{default}>` on `PaneContribution`) so the
> layout doesn't hardcode `=== 'agens'`. v1 keeps the explicit switch — it's honest and small; the generic
> registry is a follow-on, not a stub.

---

## 5. Orthogonality: how the dock mount is independent of center routing

- The agens instance lives in `workspace.layout.right.tabs` (a `persist:true` PluresDB fact) and its body is
  rendered by `WorkspaceLayout`'s `paneBody` snippet — which is a **sibling of `{@render children()}`**, NOT a
  descendant of the routed page. `WorkspaceLayout` mounts ONCE at the layout root and never unmounts on
  navigation (SvelteKit only re-renders `children`, i.e. the center `+page.svelte`).
- Therefore navigating the center (`/` → `/operations` → `/inventory` → `/admin`) swaps only the center
  snippet; the right dock's `AgensSurface` component instance is **preserved** — its `messages`, scroll,
  streaming state, and `runtimeReady` all survive (VS Code Copilot-Chat behavior).
- Lifecycle proof: `AgensSurface`'s `onMount` runs once (on first dock mount), not per center navigation,
  because Svelte keeps the same component instance while the `{#each dock.tabs}`-keyed tab (`agens#1`) stays
  in the tree.
- Dock geometry/visibility/tab-order changes persist as facts (Run B), so the pane survives full reload too.

---

## D. Decision: /agent route — stay, redirect, or thin wrapper?

**Recommendation: THIN WRAPPER around the shared `AgensSurface` (option C).**

Rationale:
- **Keep the `/agent` route** — the agens plugin still contributes its `/agent` nav item; removing the route
  would break that nav entry and any deep links/bookmarks. A redirect to "somewhere the dock is" is meaningless
  because the dock has no URL (it's orthogonal to routing) — there is nowhere to redirect *to*.
- **Make it thin** — the route becomes `<svelte:head><title>Agens — Radix</title></svelte:head><AgensSurface />`.
  This eliminates the duplication that would otherwise exist between the route and the dock (anti-dup gate) and
  keeps a single honest implementation (C-NOSTUB-001): one surface, two mount points (full-page route + dock tab).
- **No stub, no drift** — because both consume the identical component, a fix to the empty-state or the chat
  logic lands in both places at once. The route is a legitimate full-screen presentation of the same surface
  (some users may prefer the maximized view; the dock is the always-available companion).

C-NOSTUB-001 check: PASS — the surface is real (honest desktop-only state in browser, real agent-api in Tauri);
the route wrapper is a real render of it, not a placeholder; the non-agens `paneBody` branch is a truthful
EmptyState, not fabricated content.

---

## 7. Named unit test(s)

`src/lib/workspace/pane-contributions.test.ts` (vitest, real assertions — C-TEST-002):

- **`seedInstancesFromContributions places a defaultVisible agens contribution into the right dock`**
  — given `defaultLayout()` + `[{pluginId:'agens', id:'agens-console', title:'Agens', preferredDock:'right',
  defaultVisible:true, allowMultiple:true}]`, assert result has `docks.right.tabs === ['agens#1']`,
  `instances['agens#1'].pluginId === 'agens'`, and `docks.right.activeTab === 'agens#1'`.
- **`seeding is idempotent — an already-present plugin instance is not duplicated on re-seed`**
  — feed the seeded state back through `seedInstancesFromContributions` with the same contributions; assert
  `docks.right.tabs` is still `['agens#1']` (no `agens#2`).
- **`a non-defaultVisible contribution is not seeded`** — `defaultVisible:false` (or omitted) → no instance added.
- **`contributionToInstance resolves the dock through resolve_pane_dock`** — a `preferredDock:'bottom'`
  contribution lands in `bottom`; a `preferredDock:'left'` with left in `DOCKABLE` lands in `left`; an
  override that is allowed wins (delegates to Run B `resolvePaneDock`, twin of the `.px`).
- **`nextInstanceId allocates <pluginId>#<n> avoiding collisions`** — with `agens#1` present, returns `agens#2`.

(No new `.svelte` decision logic to unit-test — `AgensSurface` is the extracted existing surface, covered by
its existing browser empty-state behavior; the `paneBody` switch is trivial render glue.)

---

## 8. VERIFY assertions (real run — the C-gate, not fixtures)

1. Boot the app (dev server in browser is sufficient for the orthogonality + persistence proof; Tauri only
   changes the surface's internal state from "desktop-only empty-state" to "live chat" — orthogonality is
   identical).
2. Assert the **agens pane is visible in the RIGHT dock** on first boot: `query('workspace.layout').right.tabs`
   contains `agens#1`; the right dock renders `AgensSurface` (in browser: the "Agent runtime unavailable —
   desktop only" empty-state, honestly).
3. **Navigate the center through ≥3 routes** — e.g. `/` → `/operations` → `/inventory` → `/admin`.
4. Assert after each navigation the agens pane **STAYS MOUNTED**: the same `AgensSurface` DOM node persists
   (its `onMount` fired exactly once — assert via a mount counter or that `runtimeReady`/scroll/any typed-but-
   unsent input value is retained across the navigations). RETAINS STATE: type text into the agens input
   (or, in Tauri, send a message), navigate center, confirm the input text / message list is unchanged.
5. Assert center still renders the destination route's page each time (routing unaffected).
6. Persistence bonus (inherited from Run B): reload → agens#1 still in `right`.

PASS iff: agens visible in right dock on boot AND survives ≥3 center navigations with state intact AND
routing still works AND no fabricated content anywhere (browser shows honest empty-state).

---

## End Result: **PASS**

Analyze lane complete — NO edits made. The plan makes agens a real right-dock pane instance by:
(1) extending `RadixPlugin`/`PluginManifest` with `panes?: PaneContribution[]` (allowMultiple default true,
non-singleton per study §7); (2) agens declaring a right-dock, defaultVisible contribution; (3) a PURE-TS,
unit-tested seeder that reuses Run B `addInstance` + `resolve_pane_dock`; (4) EXTRACTING the agens surface
into a shared `AgensSurface.svelte` consumed by BOTH the `/agent` route (now a thin wrapper) AND the dock
`paneBody` snippet (no duplication, honest desktop-only empty-state in browser); (5) mounting it orthogonally
as a sibling of `{@render children()}` so it persists + retains state across center route changes.
C-NOSTUB-001: PASS.
