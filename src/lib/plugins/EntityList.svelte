<script lang="ts">
	import { onMount } from 'svelte';
	import {
		listEntities,
		searchEntities,
		deleteEntity,
	} from '$lib/plugins/plugin-api.js';
	import type { FieldInfo } from '$lib/plugins/plugin-api.js';
	import { Box, Button, Input, Table, Text } from '@plures/design-dojo';
	import EntityForm from './EntityForm.svelte';
	import EntityDetail from './EntityDetail.svelte';

	interface Props {
		pluginName: string;
		entityType: string;
		fields: FieldInfo[];
	}

	let { pluginName, entityType, fields }: Props = $props();

	// eslint-disable-next-line plures/no-raw-stores
	let items = $state<Record<string, unknown>[]>([]);
	// eslint-disable-next-line plures/no-raw-stores
	let searchQuery = $state('');
	// eslint-disable-next-line plures/no-raw-stores
	let showForm = $state(false);
	// eslint-disable-next-line plures/no-raw-stores
	let editingId = $state<string | null>(null);
	// eslint-disable-next-line plures/no-raw-stores
	let viewingId = $state<string | null>(null);
	// eslint-disable-next-line plures/no-raw-stores
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
			// eslint-disable-next-line plures/no-manual-logging
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
	// eslint-disable-next-line plures/no-raw-stores
	let visibleFields = $derived(fields.filter((f) => !f.name.startsWith('_')));
</script>

<Box class="entity-list">
	<Box class="toolbar" direction="row" gap="0.75rem">
		<Input
			type="search"
			placeholder={`Search ${entityType}…`}
			bind:value={searchQuery}
			oninput={() => load()}
		/>
		<Button variant="primary" onclick={() => { showForm = true; editingId = null; }}>
			+ Create
		</Button>
	</Box>

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
		<Text as="p" class="muted">Loading…</Text>
	{:else if items.length === 0}
		<Text as="p" class="muted">No {entityType} records yet.</Text>
	{:else}
		<Table
			columns={[
				...visibleFields.map(f => ({ key: f.name, label: f.name })),
				{ key: 'actions', label: 'Actions' }
			]}
			rows={items.map(item => ({
				...Object.fromEntries(
					visibleFields.map(f => [f.name, String(item[f.name] ?? '—')])
				),
				actions: String(item._id)
			}))}
			onselect={(index) => {
				const item = items[index];
				if (item) viewingId = item._id as string;
			}}
		/>
		<Box class="action-buttons-wrapper" direction="column" gap="0.5rem">
			{#each items as item}
				<Box class="action-buttons" direction="row" gap="0.25rem">
					<Button variant="secondary" onclick={() => { editingId = item._id as string; showForm = true; }}>✏️ Edit</Button>
					<Button variant="secondary" onclick={() => handleDelete(item._id as string)}>🗑️ Delete</Button>
				</Box>
			{/each}
		</Box>
	{/if}
</Box>

<style>
	:global(.entity-list) { margin-top: 1rem; }
	:global(.toolbar) { display: flex; gap: 0.75rem; margin-bottom: 1rem; }
	:global(.muted) { color: var(--color-text-muted); }
	:global(.entity-list .btn.secondary) {
		padding: 4px 8px;
		font-weight: 400;
	}
	:global(.entity-list td .btn.secondary) {
		width: 100%;
		justify-content: flex-start;
	}
	:global(.action-buttons) {
		display: flex;
		gap: 0.25rem;
	}
</style>
