<script lang="ts">
	/**
	 * RouteEditor — edit plugin routes, navigation items, and data requirements.
	 * Part of Design Mode Phase 3.
	 */

	import { emitFact } from '$lib/stores/praxis-svelte.js';

	interface RouteEntry {
		path: string;
		title: string;
		component: string;
		requires: { type: string; minCount: number; emptyMessage: string; fulfillHref: string; fulfillLabel: string }[];
	}

	interface NavEntry {
		href: string;
		label: string;
		icon: string;
		children: NavEntry[];
	}

	interface Props {
		pluginId: string;
		routes: RouteEntry[];
		navItems: NavEntry[];
		onSave: (routes: RouteEntry[], navItems: NavEntry[]) => void;
		onCancel: () => void;
	}

	let { pluginId, routes, navItems, onSave, onCancel }: Props = $props();

	let editRoutes = $state<RouteEntry[]>(routes.map(r => ({ ...r, requires: [...r.requires] })));
	let editNav = $state<NavEntry[]>(navItems.map(n => ({ ...n, children: [...n.children] })));
	let activeTab = $state<'routes' | 'nav'>('routes');

	function addRoute() {
		editRoutes = [...editRoutes, { path: '/', title: 'New Page', component: 'Card', requires: [] }];
	}

	function removeRoute(idx: number) {
		editRoutes = editRoutes.filter((_, i) => i !== idx);
	}

	function addNavItem() {
		editNav = [...editNav, { href: '/', label: 'New Item', icon: '📄', children: [] }];
	}

	function removeNavItem(idx: number) {
		editNav = editNav.filter((_, i) => i !== idx);
	}

	function addRequirement(routeIdx: number) {
		editRoutes[routeIdx].requires = [
			...editRoutes[routeIdx].requires,
			{ type: '', minCount: 1, emptyMessage: 'No data yet', fulfillHref: '/', fulfillLabel: 'Add data' },
		];
	}

	function handleSave() {
		onSave(editRoutes, editNav);
	}
</script>

<div class="route-editor">
	<header class="editor-header">
		<h2>🔗 Route Editor — {pluginId}</h2>
	</header>

	<div class="tab-bar">
		<button class="tab" class:active={activeTab === 'routes'} onclick={() => activeTab = 'routes'}>
			Routes ({editRoutes.length})
		</button>
		<button class="tab" class:active={activeTab === 'nav'} onclick={() => activeTab = 'nav'}>
			Navigation ({editNav.length})
		</button>
	</div>

	{#if activeTab === 'routes'}
		<div class="entries">
			{#each editRoutes as route, idx}
				<div class="entry-card">
					<div class="entry-row">
						<label>
							Path
							<input type="text" bind:value={route.path} />
						</label>
						<label>
							Title
							<input type="text" bind:value={route.title} />
						</label>
						<label>
							Component
							<input type="text" bind:value={route.component} placeholder="design-dojo component" />
						</label>
						<button class="btn-remove" onclick={() => removeRoute(idx)}>✕</button>
					</div>

					{#if route.requires.length > 0}
						<div class="requirements">
							<span class="req-label">Data Requirements:</span>
							{#each route.requires as req, reqIdx}
								<div class="req-row">
									<input type="text" bind:value={req.type} placeholder="data type" />
									<input type="number" bind:value={req.minCount} min="0" />
									<input type="text" bind:value={req.emptyMessage} placeholder="empty message" />
								</div>
							{/each}
						</div>
					{/if}

					<button class="btn-add-req" onclick={() => addRequirement(idx)}>
						+ Add Data Requirement
					</button>
				</div>
			{/each}

			<button class="btn-add" onclick={addRoute}>+ Add Route</button>
		</div>
	{:else}
		<div class="entries">
			{#each editNav as item, idx}
				<div class="entry-card">
					<div class="entry-row">
						<label>
							Icon
							<input type="text" bind:value={item.icon} style="width: 3rem;" />
						</label>
						<label>
							Label
							<input type="text" bind:value={item.label} />
						</label>
						<label>
							Href
							<input type="text" bind:value={item.href} />
						</label>
						<button class="btn-remove" onclick={() => removeNavItem(idx)}>✕</button>
					</div>
				</div>
			{/each}

			<button class="btn-add" onclick={addNavItem}>+ Add Nav Item</button>
		</div>
	{/if}

	<div class="editor-actions">
		<button class="btn-save" onclick={handleSave}>💾 Save Routes</button>
		<button class="btn-cancel" onclick={onCancel}>Cancel</button>
	</div>
</div>

<style>
	.route-editor {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 1.5rem;
	}

	.editor-header h2 { margin: 0 0 1rem; font-size: 1.1rem; }

	.tab-bar { display: flex; gap: 0.25rem; margin-bottom: 1rem; }

	.tab {
		padding: 0.4rem 0.75rem; border: 1px solid var(--color-border);
		border-radius: 4px 4px 0 0; background: transparent;
		color: var(--color-text-muted); cursor: pointer; font-size: 0.8rem;
	}
	.tab.active {
		background: var(--color-surface); color: var(--color-text);
		border-bottom-color: var(--color-surface);
	}

	.entries { display: flex; flex-direction: column; gap: 0.75rem; }

	.entry-card {
		padding: 0.75rem; border: 1px solid var(--color-border);
		border-radius: 6px; display: flex; flex-direction: column; gap: 0.5rem;
	}

	.entry-row { display: flex; gap: 0.5rem; align-items: end; }
	.entry-row label { display: flex; flex-direction: column; gap: 0.15rem; font-size: 0.75rem; color: var(--color-text-muted); flex: 1; }
	.entry-row input {
		padding: 0.35rem 0.5rem; border: 1px solid var(--color-border);
		border-radius: 4px; background: var(--color-bg); color: var(--color-text);
		font-size: 0.8rem;
	}

	.btn-remove {
		padding: 0.35rem 0.5rem; border: 1px solid var(--color-danger);
		border-radius: 4px; background: transparent; color: var(--color-danger);
		cursor: pointer; font-size: 0.8rem;
	}

	.requirements { padding: 0.5rem; background: var(--color-bg); border-radius: 4px; }
	.req-label { font-size: 0.7rem; color: var(--color-text-muted); }
	.req-row { display: flex; gap: 0.5rem; margin-top: 0.25rem; }
	.req-row input { flex: 1; padding: 0.25rem 0.4rem; border: 1px solid var(--color-border); border-radius: 3px; background: var(--color-surface); color: var(--color-text); font-size: 0.75rem; }

	.btn-add-req, .btn-add {
		padding: 0.35rem 0.75rem; border: 1px dashed var(--color-border);
		border-radius: 4px; background: transparent; color: var(--color-text-muted);
		cursor: pointer; font-size: 0.8rem;
	}
	.btn-add-req:hover, .btn-add:hover { border-color: var(--color-accent); color: var(--color-accent); }

	.editor-actions { display: flex; gap: 0.75rem; margin-top: 1rem; }
	.btn-save {
		padding: 0.5rem 1.25rem; border: none; border-radius: 6px;
		background: var(--color-accent); color: white; cursor: pointer; font-weight: 500;
	}
	.btn-cancel {
		padding: 0.5rem 1rem; border: 1px solid var(--color-border);
		border-radius: 6px; background: transparent; color: var(--color-text); cursor: pointer;
	}
</style>
