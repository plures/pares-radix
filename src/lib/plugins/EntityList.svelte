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
			on:input={() => load()}
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
		<Table>
			<svelte:element this={"thead"}>
				<svelte:element this={"tr"}>
					{#each visibleFields as field}
						<svelte:element this={"th"}>{field.name}</svelte:element>
					{/each}
					<svelte:element this={"th"}>Actions</svelte:element>
				</svelte:element>
			</svelte:element>
			<svelte:element this={"tbody"}>
				{#each items as item}
					<svelte:element this={"tr"}>
						{#each visibleFields as field}
							<svelte:element this={"td"}>
								<Button variant="secondary" onclick={() => (viewingId = item._id as string)}>
									{item[field.name] ?? '—'}
								</Button>
							</svelte:element>
						{/each}
						<svelte:element this={"td"}>
							<Box class="action-buttons" direction="row" gap="0.25rem">
								<Button variant="secondary" onclick={() => { editingId = item._id as string; showForm = true; }}>✏️</Button>
								<Button variant="secondary" onclick={() => handleDelete(item._id as string)}>🗑️</Button>
							</Box>
						</svelte:element>
					</svelte:element>
				{/each}
			</svelte:element>
		</Table>
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
