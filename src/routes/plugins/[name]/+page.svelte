<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { Box, Button, Heading, Link, Text } from '@plures/design-dojo';
	import { pluginSchema as _pluginSchema } from '$lib/plugins/plugin-api.js';
	import EntityList from '$lib/plugins/EntityList.svelte';
	import type { PluginInfo } from '$lib/plugins/plugin-api.js';
	import { plugins } from '$lib/stores/plugins.svelte.js';

	// eslint-disable-next-line plures/no-raw-stores
	let schema = $state<PluginInfo['entities']>([]);
	// eslint-disable-next-line plures/no-raw-stores
	let activeEntity = $state<string | null>(null);
	// eslint-disable-next-line plures/no-raw-stores
	let pluginName = $derived(page.params.name);

	onMount(async () => {
		await plugins.refresh();
		const info = plugins.installed.find((p) => p.name === pluginName);
		if (info) {
			schema = info.entities;
			if (schema.length > 0) activeEntity = schema[0].name;
		}
	});
</script>

<Box class="page">
	<Box class="page-header">
		<Link href="/plugins" class="back">← Plugins</Link>
		<Heading level={1} class="page-title">{pluginName}</Heading>
	</Box>

	{#if schema.length === 0}
		<Text as="p" class="muted">No entity types defined in this plugin.</Text>
	{:else}
		<Box class="entity-tabs">
			{#each schema as entity}
				<Button
					class={`tab ${activeEntity === entity.name ? 'active' : ''}`}
					variant="ghost"
					onclick={() => (activeEntity = entity.name)}
				>
					{entity.icon ?? '📄'} {entity.display_name}
				</Button>
			{/each}
		</Box>

		{#if activeEntity}
			{@const entityDef = schema.find((e) => e.name === activeEntity)}
			{#if entityDef}
				<EntityList pluginName={pluginName} entityType={activeEntity} fields={entityDef.fields} />
			{/if}
		{/if}
	{/if}
</Box>

<style>
	:global(.page) { padding: 1.5rem; max-width: 960px; }
	:global(.page-header) { display: flex; align-items: baseline; gap: 1rem; margin-bottom: 1.5rem; }
	:global(.back) { color: var(--color-text-muted); text-decoration: none; font-size: 0.9rem; }
	:global(.back:hover) { color: var(--color-accent); }
	:global(.page-title) { margin: 0; font-size: 1.5rem; }
	:global(.muted) { color: var(--color-text-muted); }
	:global(.entity-tabs) { display: flex; gap: 0.5rem; margin-bottom: 1.5rem; flex-wrap: wrap; }
	:global(.tab) {
		padding: 0.5rem 1rem; border-radius: 6px;
		background: var(--color-surface); border: 1px solid var(--color-border);
		font-size: 0.9rem;
	}
	:global(.tab.active) { border-color: var(--color-accent); background: var(--color-accent-bg); }
</style>
