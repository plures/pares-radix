<script lang="ts">
	import { onMount } from 'svelte';
	import {
		listEntities,
		searchEntities,
		deleteEntity,
	} from '$lib/plugins/plugin-api.js';
	import type { FieldInfo } from '$lib/plugins/plugin-api.js';
	import EntityForm from './EntityForm.svelte';
	import EntityDetail from './EntityDetail.svelte';

	interface Props {
		pluginName: string;
		entityType: string;
		fields: FieldInfo[];
	}

	let { pluginName, entityType, fields }: Props = $props();

	let items = $state<Record<string, unknown>[]>([]);
	let searchQuery = $state('');
	let showForm = $state(false);
	let editingId = $state<string | null>(null);
	let viewingId = $state<string | null>(null);
	let loading = $state(false);

	async function load() {
		loading = true;
		try {
			if (searchQuery.trim()) {
				items = await searchEntities(searchQuery, pluginName);
			} else {
				items = await listEntities(pluginName, entityType);
			}
		} catch (e) {
			console.error('Failed to load entities:', e);
		} finally {
			loading = false;
		}
	}

	onMount(load);

	// Reload when entity type changes
	$effect(() => {
		void entityType;
		load();
	});

	async function handleDelete(id: string) {
		if (!confirm('Delete this entity?')) return;
		await deleteEntity(id);
		await load();
	}

	function handleSaved() {
		showForm = false;
		editingId = null;
		load();
	}

	// Visible field columns (exclude internal _ fields)
	let visibleFields = $derived(fields.filter((f) => !f.name.startsWith('_')));
</script>

<div class="entity-list">
	<div class="toolbar">
		<input
			type="search"
			placeholder="Search {entityType}…"
			bind:value={searchQuery}
			oninput={() => load()}
		/>
		<button class="create-btn" onclick={() => { showForm = true; editingId = null; }}>
			+ Create
		</button>
	</div>

	{#if showForm}
		<EntityForm
			{pluginName}
			{entityType}
			{fields}
			entityId={editingId}
			initialValues={editingId ? items.find((i) => i._id === editingId) : undefined}
			onSaved={handleSaved}
			onCancel={() => { showForm = false; editingId = null; }}
		/>
	{:else if viewingId}
		{@const item = items.find((i) => i._id === viewingId)}
		{#if item}
			<EntityDetail
				{fields}
				entity={item}
				onEdit={() => { editingId = viewingId; viewingId = null; showForm = true; }}
				onDelete={() => { handleDelete(viewingId!); viewingId = null; }}
				onBack={() => (viewingId = null)}
			/>
		{/if}
	{:else if loading}
		<p class="muted">Loading…</p>
	{:else if items.length === 0}
		<p class="muted">No {entityType} records yet.</p>
	{:else}
		<table>
			<thead>
				<tr>
					{#each visibleFields as field}
						<th>{field.name}</th>
					{/each}
					<th>Actions</th>
				</tr>
			</thead>
			<tbody>
				{#each items as item}
					<tr>
						{#each visibleFields as field}
							<td>
								<button class="cell-btn" onclick={() => (viewingId = item._id as string)}>
									{item[field.name] ?? '—'}
								</button>
							</td>
						{/each}
						<td>
							<button class="action-btn" onclick={() => { editingId = item._id as string; showForm = true; }}>✏️</button>
							<button class="action-btn danger" onclick={() => handleDelete(item._id as string)}>🗑️</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</div>

<style>
	.entity-list { margin-top: 1rem; }
	.toolbar { display: flex; gap: 0.75rem; margin-bottom: 1rem; }
	.toolbar input {
		flex: 1; padding: 0.5rem 0.75rem; border-radius: 6px;
		border: 1px solid var(--color-border); background: var(--color-surface);
		color: var(--color-text); font-size: 0.9rem;
	}
	.create-btn {
		padding: 0.5rem 1rem; border-radius: 6px; cursor: pointer; border: none;
		background: var(--color-accent); color: #fff; font-weight: 500;
	}
	.muted { color: var(--color-text-muted); }
	table { width: 100%; border-collapse: collapse; }
	th, td { text-align: left; padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--color-border); }
	th { font-size: 0.8rem; color: var(--color-text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
	.cell-btn { all: unset; cursor: pointer; width: 100%; }
	.cell-btn:hover { color: var(--color-accent); }
	.action-btn { all: unset; cursor: pointer; padding: 0.25rem; }
	.action-btn.danger:hover { color: var(--color-danger); }
</style>
