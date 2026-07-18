<script lang="ts">
	/**
	 * WorkspaceLayout — the Radix workbench composition root (VS Code Panel /
	 * Secondary Sidebar model). Nests WtSplitPane to lay out center + right +
	 * bottom (+ optional left) docks; each non-center dock is a WtPane wrapping a
	 * WtPaneTabs tab group of pane instances. Pointer + keyboard DnD is wired from
	 * src/lib/panes/dnd.ts to the ondispatch(action) callback.
	 *
	 * This is the render/IO boundary ONLY: it decides nothing about docks — it
	 * renders the `layout` prop and emits WorkspaceActions via `ondispatch`. Empty
	 * visible docks render an honest EmptyState (C-NOSTUB-001) — never fake panes.
	 * The center hosts the routed page via the `children` snippet (routing
	 * unaffected). Per-instance surfaces are rendered by the `paneBody` snippet the
	 * host supplies (Run C swaps in the real agens surface).
	 */
	import type { Snippet } from 'svelte';
	import { WtSplitPane, WtPane, WtPaneTabs, EmptyState } from '@plures/design-dojo';
	import type { TabDescriptor } from '../../../src/lib/panes/types.js';
	import { beginDrag, updateDrag, endDrag, keyboardMove } from '../../../src/lib/panes/dnd.js';
	import type { DndSession, DropTarget } from '../../../src/lib/panes/types.js';
	import {
		type WorkspaceLayoutState,
		type WorkspaceAction,
		type DockId,
		type InstanceId,
		type PaneInstance,
		DOCKABLE,
	} from '../../../src/lib/workspace/types.js';

	interface Props {
		layout: WorkspaceLayoutState;
		/** The routed center page. */
		children: Snippet;
		/** Renders a docked instance's real surface. Run B passes an honest EmptyState-per-instance. */
		paneBody?: Snippet<[PaneInstance]>;
		ondispatch?: (action: WorkspaceAction) => void;
	}

	let { layout, children, paneBody, ondispatch }: Props = $props();

	function tabsFor(dock: DockId): TabDescriptor[] {
		return layout.docks[dock].tabs.map((id) => {
			const inst = layout.instances[id];
			return { id, title: inst?.title ?? id, closable: true };
		});
	}

	// ── DnD state (cross-dock). Intra-dock reorder is handled by WtPaneTabs. ────
	let session = $state<DndSession | null>(null);

	/** Map a pointer position to a dock+index using data-dock hit regions. */
	function hitTest(x: number, y: number): DropTarget | null {
		const el = document.elementFromPoint(x, y);
		const dockEl = el?.closest<HTMLElement>('[data-dock]');
		if (!dockEl) return null;
		const dockId = dockEl.dataset.dock as DockId;
		// Append at end by default (-1); WtPaneTabs owns fine-grained ordering.
		return { dockId, index: -1 };
	}

	function onTabPointerDown(e: PointerEvent, dock: DockId, id: InstanceId) {
		// Left button only; let WtPaneTabs handle selection/close.
		if (e.button !== 0) return;
		session = beginDrag({ id, fromDock: dock }, e.clientX, e.clientY);
	}

	function onWindowPointerMove(e: PointerEvent) {
		if (!session?.active) return;
		session = updateDrag(session, e.clientX, e.clientY, hitTest);
	}

	function onWindowPointerUp() {
		if (!session?.active) return;
		const cmd = endDrag(session);
		session = null;
		if (cmd && cmd.toDock !== cmd.fromDock) {
			ondispatch?.({
				type: 'moveInstance',
				instanceId: cmd.itemId,
				toDock: cmd.toDock as DockId,
				toIndex: cmd.toIndex,
			});
		}
	}

	function onTabKeyMove(e: KeyboardEvent, dock: DockId, id: InstanceId) {
		if (!(e.ctrlKey || e.metaKey)) return; // Ctrl/Cmd+Arrow moves across docks
		const cmd = keyboardMove({ id, fromDock: dock }, e.key, DOCKABLE, dock);
		if (cmd) {
			e.preventDefault();
			ondispatch?.({
				type: 'moveInstance',
				instanceId: cmd.itemId,
				toDock: cmd.toDock as DockId,
				toIndex: cmd.toIndex,
			});
		}
	}

	const rightVisible = $derived(layout.docks.right.visible);
	const bottomVisible = $derived(layout.docks.bottom.visible);
	const leftVisible = $derived(layout.docks.left.visible);
</script>

<svelte:window onpointermove={onWindowPointerMove} onpointerup={onWindowPointerUp} />

{#snippet dockRegion(dock: DockId)}
	{@const d = layout.docks[dock]}
	<div class="dock" data-dock={dock}>
		<WtPane
			title={dock}
			collapsed={!d.visible}
			oncollapse={(collapsed) => ondispatch?.({ type: 'toggleDock', dock, visible: !collapsed })}
		>
			{#if d.tabs.length === 0}
				<EmptyState title="No panes docked here" description={`Drag a pane into the ${dock} dock.`} />
			{:else}
				<WtPaneTabs
					tabs={tabsFor(dock)}
					active={d.activeTab}
					onselect={(id) => ondispatch?.({ type: 'setActive', dock, instanceId: id })}
					onclose={(id) => ondispatch?.({ type: 'removeInstance', instanceId: id })}
					onreorder={(tabs) => {
						const from = d.tabs.indexOf(d.activeTab ?? d.tabs[0]);
						const to = tabs.findIndex((t) => t.id === (d.activeTab ?? d.tabs[0]));
						if (from !== -1 && to !== -1 && from !== to) {
							ondispatch?.({ type: 'reorderInDock', dock, from, to });
						}
					}}
				>
					{#snippet panel(id)}
						<div
							class="dock-tab-grip"
							role="button"
							tabindex="0"
							aria-label={`Move ${layout.instances[id]?.title ?? id} between docks`}
							onpointerdown={(e) => onTabPointerDown(e, dock, id)}
							onkeydown={(e) => onTabKeyMove(e, dock, id)}
						>
							{#if paneBody}
								{@render paneBody(layout.instances[id])}
							{:else}
								<EmptyState
									title={layout.instances[id]?.title ?? id}
									description="Pane surface unavailable in this build."
								/>
							{/if}
						</div>
					{/snippet}
				</WtPaneTabs>
			{/if}
		</WtPane>
	</div>
{/snippet}

<div class="workspace">
	<WtSplitPane
		orientation="horizontal"
		size={leftVisible ? layout.docks.left.size : 0}
		collapsed={!leftVisible}
		onresize={(size) => ondispatch?.({ type: 'resizeDock', dock: 'left', size })}
	>
		{#snippet a()}
			{#if leftVisible}{@render dockRegion('left')}{/if}
		{/snippet}
		{#snippet b()}
			<!-- Vertical split: A = bottom dock (sized to bottom.size), B = top
			     region (center+right) which flex-fills. `reverse` flips the visual
			     order so the sized dock sits at the BOTTOM while center fills above. -->
			<div class="split-wrap reverse-col">
				<WtSplitPane
					orientation="vertical"
					size={bottomVisible ? layout.docks.bottom.size : 0}
					minSize={120}
					collapsed={!bottomVisible}
					onresize={(size) => ondispatch?.({ type: 'resizeDock', dock: 'bottom', size })}
				>
					{#snippet a()}
						{#if bottomVisible}{@render dockRegion('bottom')}{/if}
					{/snippet}
					{#snippet b()}
						<!-- Horizontal split: A = right dock (sized to right.size), B =
						     center which flex-fills. `reverse` puts the dock on the RIGHT. -->
						<div class="split-wrap reverse-row">
							<WtSplitPane
								orientation="horizontal"
								size={rightVisible ? layout.docks.right.size : 0}
								minSize={200}
								collapsed={!rightVisible}
								onresize={(size) => ondispatch?.({ type: 'resizeDock', dock: 'right', size })}
							>
								{#snippet a()}
									{#if rightVisible}{@render dockRegion('right')}{/if}
								{/snippet}
								{#snippet b()}
									<div class="center" data-dock="center">
										{@render children()}
									</div>
								{/snippet}
							</WtSplitPane>
						</div>
					{/snippet}
				</WtSplitPane>
			</div>
		{/snippet}
	</WtSplitPane>
</div>

<style>
	.workspace {
		display: flex;
		flex: 1 1 auto;
		min-width: 0;
		min-height: 0;
		height: 100%;
	}
	.center {
		display: flex;
		flex-direction: column;
		flex: 1 1 auto;
		min-width: 0;
		min-height: 0;
		height: 100%;
		overflow: auto;
	}
	/* split-wrap fills the parent split cell so the nested WtSplitPane can
	   measure real width/height (otherwise the sash resize math sees 0). */
	.split-wrap {
		flex: 1 1 auto;
		min-width: 0;
		min-height: 0;
		height: 100%;
		width: 100%;
		display: flex;
	}
	/* Reverse visual order so the SIZED pane A (the dock) sits after the
	   flex-filling center: bottom dock at the bottom, right dock on the right. */
	.reverse-col :global(.wt-split.vertical) {
		flex-direction: column-reverse;
	}
	.reverse-row :global(.wt-split) {
		flex-direction: row-reverse;
	}
	.dock {
		height: 100%;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}
	.dock-tab-grip {
		height: 100%;
		min-height: 0;
		touch-action: none;
	}
</style>
