<script lang="ts">
	import { onMount } from 'svelte';
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
			console.error('Install failed:', e);
		}
	}

	async function handleUninstall(name: string) {
		if (!confirm(`Uninstall plugin "${name}"?`)) return;
		await plugins.uninstall(name);
	}
</script>

<div class="page">
	<header class="page-header">
		<h1>🧩 Plugins</h1>
		<label class="install-btn">
			Install Plugin
			<input bind:this={fileInput} type="file" accept=".toml" onchange={handleInstall} hidden />
		</label>
	</header>

	{#if plugins.loading}
		<p class="muted">Loading plugins…</p>
	{:else if plugins.installed.length === 0}
		<div class="empty-state">
			<p>No plugins installed yet.</p>
			<p class="muted">Install a plugin TOML manifest to get started.</p>
		</div>
	{:else}
		<div class="plugin-grid">
			{#each plugins.installed as plugin}
				<div
					class="plugin-card"
					role="button"
					tabindex="0"
					onclick={() => goto(`/plugins/${plugin.name}`)}
					onkeydown={(e) => { if (e.key === 'Enter') goto(`/plugins/${plugin.name}`); }}
				>
					<div class="plugin-header">
						<h2>{plugin.name}</h2>
						<span class="version">v{plugin.version}</span>
					</div>
					<p class="description">{plugin.description}</p>
					<div class="plugin-meta">
						<span class="entity-count">
							{plugin.entities.length} {plugin.entities.length === 1 ? 'entity' : 'entities'}
						</span>
						<button
							class="uninstall-btn"
							onclick={(e: MouseEvent) => { e.stopPropagation(); handleUninstall(plugin.name); }}
						>
							Uninstall
						</button>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.page { padding: 1.5rem; max-width: 960px; }
	.page-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 1.5rem; }
	.page-header h1 { margin: 0; font-size: 1.5rem; }
	.install-btn {
		padding: 0.5rem 1rem; border-radius: 6px; cursor: pointer;
		background: var(--color-accent); color: #fff; font-weight: 500;
	}
	.muted { color: var(--color-text-muted); }
	.empty-state { text-align: center; padding: 3rem 1rem; }
	.plugin-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 1rem; }
	.plugin-card {
		all: unset; cursor: pointer; display: block; padding: 1.25rem; border-radius: 8px;
		background: var(--color-surface); border: 1px solid var(--color-border);
		transition: border-color 0.15s;
	}
	.plugin-card:hover { border-color: var(--color-accent); }
	.plugin-header { display: flex; align-items: baseline; gap: 0.5rem; margin-bottom: 0.5rem; }
	.plugin-header h2 { margin: 0; font-size: 1.1rem; }
	.version { font-size: 0.8rem; color: var(--color-text-muted); }
	.description { color: var(--color-text-muted); font-size: 0.9rem; margin: 0 0 0.75rem; }
	.plugin-meta { display: flex; align-items: center; justify-content: space-between; }
	.entity-count { font-size: 0.8rem; color: var(--color-text-muted); }
	.uninstall-btn {
		all: unset; cursor: pointer; font-size: 0.8rem; color: var(--color-danger);
		padding: 0.25rem 0.5rem; border-radius: 4px;
	}
	.uninstall-btn:hover { background: rgba(220, 38, 38, 0.1); }
</style>
