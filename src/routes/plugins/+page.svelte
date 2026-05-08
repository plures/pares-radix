<script lang="ts">
	import { onMount } from 'svelte';
	import { Box, Button, Heading, Input, Text } from '@plures/design-dojo';
	import { plugins } from '$lib/stores/plugins.js';
	import { goto } from '$app/navigation';

	let fileInput: HTMLInputElement;

	onMount(() => {
		plugins.refresh();
	});

	async function handleInstall() {
		const file = fileInput?.files?.[0];
		if (!file) return;
		try {
			// Tauri needs a file path, not a File object.
			// For now we use the file name — in production this would use
			// tauri-plugin-dialog to get the real path.
			await plugins.install(file.name);
		} catch (e) {
			// eslint-disable-next-line plures/no-manual-logging
			console.error('Install failed:', e);
		}
	}

	async function handleUninstall(name: string) {
		if (!confirm(`Uninstall plugin "${name}"?`)) return;
		await plugins.uninstall(name);
	}
</script>

<Box class="page">
	<Box class="page-header">
		<Heading level={1} class="page-title">🧩 Plugins</Heading>
		<Input
			class="install-input"
			label="Install Plugin"
			bind:this={fileInput}
			type="file"
			accept=".toml"
			onchange={handleInstall}
		/>
	</Box>

	{#if plugins.loading}
		<Text as="p" class="muted">Loading plugins…</Text>
	{:else if plugins.installed.length === 0}
		<Box class="empty-state">
			<Text as="p">No plugins installed yet.</Text>
			<Text as="p" class="muted">Install a plugin TOML manifest to get started.</Text>
		</Box>
	{:else}
		<Box class="plugin-grid">
			{#each plugins.installed as plugin}
				<Box
					class="plugin-card"
					role="button"
					tabindex="0"
					onclick={() => goto(`/plugins/${plugin.name}`)}
					onkeydown={(e) => { if (e.key === 'Enter') goto(`/plugins/${plugin.name}`); }}
				>
					<Box class="plugin-header">
						<Heading level={2} class="plugin-title">{plugin.name}</Heading>
						<Text as="span" class="version">v{plugin.version}</Text>
					</Box>
					<Text as="p" class="description">{plugin.description}</Text>
					<Box class="plugin-meta">
						<Text as="span" class="entity-count">
							{plugin.entities.length} {plugin.entities.length === 1 ? 'entity' : 'entities'}
						</Text>
						<Button
							class="uninstall-btn"
							variant="ghost"
							onclick={(e: MouseEvent) => { e.stopPropagation(); handleUninstall(plugin.name); }}
						>
							Uninstall
						</Button>
					</Box>
				</Box>
			{/each}
		</Box>
	{/if}
</Box>

<style>
	:global(.page) { padding: 1.5rem; max-width: 960px; }
	:global(.page-header) { display: flex; align-items: center; justify-content: space-between; margin-bottom: 1.5rem; gap: 1rem; }
	:global(.page-title) { margin: 0; font-size: 1.5rem; }
	:global(.install-input) { max-width: 280px; }
	:global(.muted) { color: var(--color-text-muted); }
	:global(.empty-state) { text-align: center; padding: 3rem 1rem; }
	:global(.plugin-grid) { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 1rem; }
	:global(.plugin-card) {
		cursor: pointer; display: block; padding: 1.25rem; border-radius: 8px;
		background: var(--color-surface); border: 1px solid var(--color-border);
		transition: border-color 0.15s;
	}
	:global(.plugin-card:hover) { border-color: var(--color-accent); }
	:global(.plugin-header) { display: flex; align-items: baseline; gap: 0.5rem; margin-bottom: 0.5rem; }
	:global(.plugin-title) { margin: 0; font-size: 1.1rem; }
	:global(.version) { font-size: 0.8rem; color: var(--color-text-muted); }
	:global(.description) { color: var(--color-text-muted); font-size: 0.9rem; margin: 0 0 0.75rem; }
	:global(.plugin-meta) { display: flex; align-items: center; justify-content: space-between; }
	:global(.entity-count) { font-size: 0.8rem; color: var(--color-text-muted); }
	:global(.uninstall-btn) {
		font-size: 0.8rem; color: var(--color-danger);
		padding: 0.25rem 0.5rem; border-radius: 4px;
	}
	:global(.uninstall-btn:hover) { background: rgba(220, 38, 38, 0.1); }
</style>
