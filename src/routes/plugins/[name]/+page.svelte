<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { pluginSchema as _pluginSchema } from '$lib/plugins/plugin-api.js';
	import EntityList from '$lib/plugins/EntityList.svelte';
	import type { PluginInfo } from '$lib/plugins/plugin-api.js';
	import { plugins } from '$lib/stores/plugins.js';

	let schema = $state<PluginInfo['entities']>([]);
	let activeEntity = $state<string | null>(null);
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

<div class="page">
	<header class="page-header">
		<a href="/plugins" class="back">← Plugins</a>
		<h1>{pluginName}</h1>
	</header>

	{#if schema.length === 0}
		<p class="muted">No entity types defined in this plugin.</p>
	{:else}
		<nav class="entity-tabs">
			{#each schema as entity}
				<button
					class="tab"
					class:active={activeEntity === entity.name}
					onclick={() => (activeEntity = entity.name)}
				>
					{entity.icon ?? '📄'} {entity.display_name}
				</button>
			{/each}
		</nav>

		{#if activeEntity}
			{@const entityDef = schema.find((e) => e.name === activeEntity)}
			{#if entityDef}
				<EntityList pluginName={pluginName} entityType={activeEntity} fields={entityDef.fields} />
			{/if}
		{/if}
	{/if}
</div>

<style>
	.page { padding: 1.5rem; max-width: 960px; }
	.page-header { display: flex; align-items: baseline; gap: 1rem; margin-bottom: 1.5rem; }
	.back { color: var(--color-text-muted); text-decoration: none; font-size: 0.9rem; }
	.back:hover { color: var(--color-accent); }
	.page-header h1 { margin: 0; font-size: 1.5rem; }
	.muted { color: var(--color-text-muted); }
	.entity-tabs { display: flex; gap: 0.5rem; margin-bottom: 1.5rem; flex-wrap: wrap; }
	.tab {
		all: unset; cursor: pointer; padding: 0.5rem 1rem; border-radius: 6px;
		background: var(--color-surface); border: 1px solid var(--color-border);
		font-size: 0.9rem;
	}
	.tab.active { border-color: var(--color-accent); background: var(--color-accent-bg); }
</style>
