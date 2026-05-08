<script lang="ts">
	/**
	 * RouteEditor — edit plugin routes, navigation items, and data requirements.
	 * Part of Design Mode Phase 3.
	 */

	import { untrack } from 'svelte';
	import { Box, Heading, Text, Button, Input } from '@plures/design-dojo';

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

	// eslint-disable-next-line plures/no-raw-stores
	let editRoutes = $state<RouteEntry[]>(untrack(() => routes.map(r => ({ ...r, requires: [...r.requires] }))));
	// eslint-disable-next-line plures/no-raw-stores
	let editNav = $state<NavEntry[]>(untrack(() => navItems.map(n => ({ ...n, children: [...n.children] }))));
	// eslint-disable-next-line plures/no-raw-stores
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

<Box class="route-editor">
	<Box as="header" class="editor-header">
		<Heading level={2}>🔗 Route Editor — {pluginId}</Heading>
	</Box>

	<Box class="tab-bar">
		<Button
			class={activeTab === 'routes' ? 'tab active' : 'tab'}
			onclick={() => activeTab = 'routes'}
		>
			Routes ({editRoutes.length})
		</Button>
		<Button
			class={activeTab === 'nav' ? 'tab active' : 'tab'}
			onclick={() => activeTab = 'nav'}
		>
			Navigation ({editNav.length})
		</Button>
	</Box>

	{#if activeTab === 'routes'}
		<Box class="entries">
			{#each editRoutes as route, idx}
				<Box class="entry-card">
					<Box class="entry-row">
						<Input class="field-input" label="Path" type="text" bind:value={route.path} />
						<Input class="field-input" label="Title" type="text" bind:value={route.title} />
						<Input
							class="field-input"
							label="Component"
							type="text"
							bind:value={route.component}
							placeholder="design-dojo component"
						/>
						<Button class="btn-remove" onclick={() => removeRoute(idx)}>✕</Button>
					</Box>

					{#if route.requires.length > 0}
						<Box class="requirements">
							<Text as="span" class="req-label">Data Requirements:</Text>
							{#each route.requires as req}
								<Box class="req-row">
									<Input class="req-input" type="text" bind:value={req.type} placeholder="data type" />
									<Input class="req-input" type="number" bind:value={req.minCount} min="0" />
									<Input
										class="req-input"
										type="text"
										bind:value={req.emptyMessage}
										placeholder="empty message"
									/>
								</Box>
							{/each}
						</Box>
					{/if}

					<Button class="btn-add-req" onclick={() => addRequirement(idx)}>
						+ Add Data Requirement
					</Button>
				</Box>
			{/each}

			<Button class="btn-add" onclick={addRoute}>+ Add Route</Button>
		</Box>
	{:else}
		<Box class="entries">
			{#each editNav as item, idx}
				<Box class="entry-card">
					<Box class="entry-row">
						<Input class="field-input" label="Icon" type="text" bind:value={item.icon} />
						<Input class="field-input" label="Label" type="text" bind:value={item.label} />
						<Input class="field-input" label="Href" type="text" bind:value={item.href} />
						<Button class="btn-remove" onclick={() => removeNavItem(idx)}>✕</Button>
					</Box>
				</Box>
			{/each}

			<Button class="btn-add" onclick={addNavItem}>+ Add Nav Item</Button>
		</Box>
	{/if}

	<Box class="editor-actions">
		<Button class="btn-save" onclick={handleSave}>💾 Save Routes</Button>
		<Button class="btn-cancel" onclick={onCancel}>Cancel</Button>
	</Box>
</Box>

<style>
	:global(.route-editor) {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 1.5rem;
	}

	:global(.editor-header h2) { margin: 0 0 1rem; font-size: 1.1rem; }

	:global(.tab-bar) { display: flex; gap: 0.25rem; margin-bottom: 1rem; }

	:global(.tab) {
		padding: 0.4rem 0.75rem; border: 1px solid var(--color-border);
		border-radius: 4px 4px 0 0; background: transparent;
		color: var(--color-text-muted); cursor: pointer; font-size: 0.8rem;
	}
	:global(.tab.active) {
		background: var(--color-surface); color: var(--color-text);
		border-bottom-color: var(--color-surface);
	}

	:global(.entries) { display: flex; flex-direction: column; gap: 0.75rem; }

	:global(.entry-card) {
		padding: 0.75rem; border: 1px solid var(--color-border);
		border-radius: 6px; display: flex; flex-direction: column; gap: 0.5rem;
	}

	:global(.entry-row) { display: flex; gap: 0.5rem; align-items: end; }

	:global(.field-input) { flex: 1; }
	:global(.req-input) { flex: 1; }

	:global(.btn-remove) {
		padding: 0.35rem 0.5rem; border: 1px solid var(--color-danger);
		border-radius: 4px; background: transparent; color: var(--color-danger);
		cursor: pointer; font-size: 0.8rem;
	}

	:global(.requirements) { padding: 0.5rem; background: var(--color-bg); border-radius: 4px; }
	:global(.req-label) { font-size: 0.7rem; color: var(--color-text-muted); }
	:global(.req-row) { display: flex; gap: 0.5rem; margin-top: 0.25rem; }

	:global(.btn-add-req), :global(.btn-add) {
		padding: 0.35rem 0.75rem; border: 1px dashed var(--color-border);
		border-radius: 4px; background: transparent; color: var(--color-text-muted);
		cursor: pointer; font-size: 0.8rem;
	}
	:global(.btn-add-req:hover), :global(.btn-add:hover) { border-color: var(--color-accent); color: var(--color-accent); }

	:global(.editor-actions) { display: flex; gap: 0.75rem; margin-top: 1rem; }
	:global(.btn-save) {
		padding: 0.5rem 1.25rem; border: none; border-radius: 6px;
		background: var(--color-accent); color: white; cursor: pointer; font-weight: 500;
	}
	:global(.btn-cancel) {
		padding: 0.5rem 1rem; border: 1px solid var(--color-border);
		border-radius: 6px; background: transparent; color: var(--color-text); cursor: pointer;
	}
</style>
