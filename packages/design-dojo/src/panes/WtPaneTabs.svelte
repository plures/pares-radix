<script lang="ts">
	/**
	 * WtPaneTabs — a real tab strip with select / close / drag-reorder, backed by
	 * the pure tabs.ts logic. Implements roving tabindex + ARIA tablist/tab/tabpanel
	 * and real active-view switching (only the active tab's panel snippet renders).
	 */
	import type { Snippet } from 'svelte';
	import type { TabDescriptor } from '../../../../src/lib/panes/types.js';
	import { reorder, closeTab, rovingNext } from '../../../../src/lib/panes/tabs.js';

	interface Props {
		tabs: TabDescriptor[];
		active?: string | null;
		/** Renders the panel body for a given tab id. */
		panel: Snippet<[string]>;
		onselect?: (id: string) => void;
		onclose?: (id: string, tabs: TabDescriptor[], active: string | null) => void;
		onreorder?: (tabs: TabDescriptor[]) => void;
	}

	let {
		tabs = $bindable([]),
		active = $bindable(null),
		panel,
		onselect,
		onclose,
		onreorder
	}: Props = $props();

	// Ensure an active tab exists when possible.
	$effect(() => {
		if (tabs.length > 0 && (active === null || !tabs.some((t) => t.id === active))) {
			active = tabs[0].id;
		}
		if (tabs.length === 0 && active !== null) {
			active = null;
		}
	});

	let dragIndex = $state<number | null>(null);

	function select(id: string) {
		active = id;
		onselect?.(id);
	}

	function close(id: string, e: Event) {
		e.stopPropagation();
		const res = closeTab(tabs, id, active);
		tabs = res.tabs;
		active = res.active;
		onclose?.(id, res.tabs, res.active);
	}

	function onTabKeydown(e: KeyboardEvent, id: string) {
		if (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Home' || e.key === 'End') {
			const next = rovingNext(tabs, active, e.key);
			if (next !== null) {
				select(next);
				focusTab(next);
				e.preventDefault();
			}
		} else if (e.key === 'Delete' || e.key === 'Backspace') {
			const tab = tabs.find((t) => t.id === id);
			if (tab?.closable !== false) {
				close(id, e);
				e.preventDefault();
			}
		}
	}

	function focusTab(id: string) {
		queueMicrotask(() => {
			const el = document.getElementById(`wt-tab-${id}`);
			el?.focus();
		});
	}

	function onDragStart(index: number) {
		dragIndex = index;
	}
	function onDragOver(e: DragEvent, overIndex: number) {
		if (dragIndex === null || dragIndex === overIndex) return;
		e.preventDefault();
	}
	function onDrop(e: DragEvent, overIndex: number) {
		e.preventDefault();
		if (dragIndex === null) return;
		tabs = reorder(tabs, dragIndex, overIndex);
		onreorder?.(tabs);
		dragIndex = null;
	}
	function onDragEnd() {
		dragIndex = null;
	}
</script>

<div class="wt-tabs">
	<div class="wt-tabstrip" role="tablist" aria-orientation="horizontal">
		{#each tabs as tab, i (tab.id)}
			<div
				id={`wt-tab-${tab.id}`}
				class="wt-tab"
				class:active={tab.id === active}
				role="tab"
				tabindex={tab.id === active ? 0 : -1}
				aria-selected={tab.id === active}
				aria-controls={`wt-tabpanel-${tab.id}`}
				draggable="true"
				onclick={() => select(tab.id)}
				onkeydown={(e) => onTabKeydown(e, tab.id)}
				ondragstart={() => onDragStart(i)}
				ondragover={(e) => onDragOver(e, i)}
				ondrop={(e) => onDrop(e, i)}
				ondragend={onDragEnd}
			>
				{#if tab.icon}<span class="wt-tab-icon" aria-hidden="true">{tab.icon}</span>{/if}
				<span class="wt-tab-title">{tab.title}</span>
				{#if tab.closable !== false}
					<button
						type="button"
						class="wt-tab-close"
						aria-label={`Close ${tab.title}`}
						onclick={(e) => close(tab.id, e)}
					>×</button>
				{/if}
			</div>
		{/each}
	</div>

	{#if active !== null}
		<div
			id={`wt-tabpanel-${active}`}
			class="wt-tabpanel"
			role="tabpanel"
			aria-labelledby={`wt-tab-${active}`}
			tabindex="0"
		>
			{@render panel(active)}
		</div>
	{/if}
</div>

<style>
	.wt-tabs {
		display: flex;
		flex-direction: column;
		min-height: 0;
		height: 100%;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		overflow: hidden;
	}
	.wt-tabstrip {
		display: flex;
		align-items: stretch;
		background: var(--color-surface-alt, var(--color-surface));
		border-bottom: 1px solid var(--color-border);
		overflow-x: auto;
	}
	.wt-tab {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		font-size: 0.8rem;
		color: var(--color-text-muted, var(--color-text));
		border-right: 1px solid var(--color-border);
		cursor: pointer;
		user-select: none;
		white-space: nowrap;
	}
	.wt-tab:hover {
		background: var(--color-border);
	}
	.wt-tab.active {
		color: var(--color-text);
		background: var(--color-surface);
		box-shadow: inset 0 -2px 0 var(--color-accent, var(--color-primary, #7c8cff));
	}
	.wt-tab:focus-visible {
		outline: 2px solid var(--color-accent, var(--color-primary, #7c8cff));
		outline-offset: -2px;
	}
	.wt-tab-close {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		padding: 0;
		border: none;
		background: transparent;
		color: inherit;
		cursor: pointer;
		border-radius: 3px;
		font-size: 0.9rem;
		line-height: 1;
	}
	.wt-tab-close:hover {
		background: var(--color-border);
	}
	.wt-tabpanel {
		flex: 1 1 auto;
		min-height: 0;
		overflow: auto;
		padding: 10px;
		color: var(--color-text);
	}
</style>
