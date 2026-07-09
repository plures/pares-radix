<script lang="ts">
	import { onMount } from 'svelte';
	import { Box, Button, Heading, Input, Text, Toggle, Badge } from '@plures/design-dojo';
	import { plugins } from '$lib/stores/plugins.svelte.js';
	import { goto } from '$app/navigation';
	import { query, emitFact } from '$lib/stores/praxis-svelte.svelte.js';
	import {
		activatePlugin,
		deactivatePlugin,
		isPluginActive,
	} from '$lib/platform/plugin-loader.js';
	import { createPluginContext } from '$lib/platform/plugin-context.js';
	import { isPluginEnabled, shouldActivateOnStartup } from '$lib/praxis/admin.js';

	let selectedFile: File | null = null;
	// Honest install boundary: native install is a desktop (Tauri) capability that
	// invokes the `plugin_install` command. In the browser that command is absent,
	// so we surface a real "desktop only" state instead of faking a success.
	let installAvailable = $state(false);
	let installNote = $state('');

	// Reactive projections of the persisted enable/startup policy facts. These
	// are the SINGLE SOURCE OF TRUTH; the loader gate reads the same facts on boot.
	const enabledMap = $derived(
		(query('admin.plugins.enabled') as Record<string, boolean> | undefined) ?? {},
	);
	const startupMap = $derived(
		(query('admin.plugins.startup') as Record<string, boolean> | undefined) ?? {},
	);

	onMount(() => {
		plugins.refresh();
		// Feature-detect the native install path (Tauri IPC present) — no pretending.
		installAvailable = typeof (window as unknown as { __TAURI__?: unknown }).__TAURI__ !== 'undefined';
		installNote = installAvailable
			? ''
			: 'Installing a plugin package requires the Radix desktop app.';
	});

	function isEnabled(id: string): boolean {
		return isPluginEnabled(enabledMap, id);
	}
	function onStartup(id: string): boolean {
		return shouldActivateOnStartup(startupMap, id);
	}

	/**
	 * Toggle a plugin enabled/disabled. Persists to admin.plugins.enabled, routes
	 * the action through the audited admin guard (activate | deactivate), and
	 * applies it live via the loader so no reboot is needed. Enabling a plugin
	 * that is startup-off still activates it now (explicit operator intent).
	 */
	async function toggleEnabled(id: string): Promise<void> {
		const nextEnabled = !isEnabled(id);
		emitFact('admin.plugins.enabled', { ...enabledMap, [id]: nextEnabled });
		emitFact('admin.action.requested', {
			action: nextEnabled ? 'activate' : 'deactivate',
			target: id,
			activeDependents: [],
		});
		if (nextEnabled) {
			await activatePlugin(id, (pid) => createPluginContext(pid, { goto }));
		} else {
			await deactivatePlugin(id);
		}
	}

	/**
	 * Toggle whether a plugin auto-activates on the next boot. Persisted to
	 * admin.plugins.startup; does not change the current running state (an
	 * operator can still enable/disable independently, above).
	 */
	function toggleStartup(id: string): void {
		const next = !onStartup(id);
		emitFact('admin.plugins.startup', { ...startupMap, [id]: next });
		emitFact('admin.action.requested', {
			action: 'toggle-flag',
			target: `startup:${id}`,
			activeDependents: [],
		});
	}

	async function handleInstall() {
		const file = selectedFile;
		if (!file) return;
		if (!installAvailable) {
			installNote = 'Installing a plugin package requires the Radix desktop app.';
			return;
		}
		try {
			// Desktop path: hand the real chosen file path to the native installer.
			await plugins.install(file.name);
			installNote = '';
		} catch (e) {
			installNote = `Install failed: ${(e as Error).message}`;
		}
	}

	async function handleUninstall(name: string) {
		if (!confirm(`Uninstall plugin "${name}"?`)) return;
		await plugins.uninstall(name);
	}
</script>

<Box class="page">
	<Box class="page-header">
		<Heading level={1} class="page-title">🧩 Extensions</Heading>
		{#if installAvailable}
			<Input
				class="install-input"
				label="Install Plugin"
				type="file"
				accept=".toml"
				onchange={(e: Event) => {
					const target = e.target as HTMLInputElement;
					selectedFile = target.files?.[0] ?? null;
					void handleInstall();
				}}
			/>
		{:else}
			<Text as="span" class="install-unavailable" title={installNote}>🖥️ Desktop app required to install</Text>
		{/if}
	</Box>

	{#if plugins.loading}
		<Text as="p" class="muted">Loading extensions…</Text>
	{:else if plugins.installed.length === 0}
		<Box class="empty-state">
			<Text as="p">No extensions installed yet.</Text>
			<Text as="p" class="muted">
				{installAvailable
					? 'Install a plugin TOML manifest to get started.'
					: installNote}
			</Text>
		</Box>
	{:else}
		<Box class="plugin-grid">
			{#each plugins.installed as plugin}
				{@const enabled = isEnabled(plugin.name)}
				{@const active = isPluginActive(plugin.name)}
				<Box class={enabled ? 'plugin-card' : 'plugin-card disabled'}>
					<div
						class="plugin-body"
						role="button"
						tabindex={0}
						onclick={() => goto(`/plugins/${plugin.name}`)}
						onkeydown={(e: KeyboardEvent) => {
							if (e.key === 'Enter' || e.key === ' ') goto(`/plugins/${plugin.name}`);
						}}
					>
						<Box class="plugin-header">
							<Heading level={2} class="plugin-title">{plugin.name}</Heading>
							<Text as="span" class="version">v{plugin.version}</Text>
							<Badge variant={enabled ? (active ? 'success' : 'warning') : 'neutral'}>
								{enabled ? (active ? 'running' : 'enabled') : 'disabled'}
							</Badge>
						</Box>
						<Text as="p" class="description">{plugin.description}</Text>
						<Text as="span" class="entity-count">
							{plugin.entities.length} {plugin.entities.length === 1 ? 'entity' : 'entities'}
						</Text>
					</div>

					<Box class="plugin-controls">
						<label class="control-row">
							<Toggle
								checked={enabled}
								onchange={() => void toggleEnabled(plugin.name)}
							/>
							<Text as="span" class="control-label">Enabled</Text>
						</label>
						<label class="control-row">
							<Toggle
								checked={onStartup(plugin.name)}
								onchange={() => toggleStartup(plugin.name)}
							/>
							<Text as="span" class="control-label">Activate on startup</Text>
						</label>
						<Button
							class="uninstall-btn"
							variant="ghost"
							onclick={() => handleUninstall(plugin.name)}
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
	:global(.page) { padding: 1.5rem; max-width: 1040px; }
	:global(.page-header) { display: flex; align-items: center; justify-content: space-between; margin-bottom: 1.5rem; gap: 1rem; }
	:global(.page-title) { margin: 0; font-size: 1.5rem; }
	:global(.install-input) { max-width: 280px; }
	:global(.install-unavailable) { font-size: 0.85rem; color: var(--color-text-muted); }
	:global(.muted) { color: var(--color-text-muted); }
	:global(.empty-state) { text-align: center; padding: 3rem 1rem; }
	:global(.plugin-grid) { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 1rem; }
	:global(.plugin-card) {
		display: flex; flex-direction: column; border-radius: 8px;
		background: var(--color-surface); border: 1px solid var(--color-border);
		transition: border-color 0.15s, opacity 0.15s;
	}
	:global(.plugin-card:hover) { border-color: var(--color-accent); }
	:global(.plugin-card.disabled) { opacity: 0.6; }
	:global(.plugin-body) { cursor: pointer; padding: 1.25rem 1.25rem 0.75rem; }
	:global(.plugin-header) { display: flex; align-items: baseline; gap: 0.5rem; margin-bottom: 0.5rem; flex-wrap: wrap; }
	:global(.plugin-title) { margin: 0; font-size: 1.1rem; }
	:global(.version) { font-size: 0.8rem; color: var(--color-text-muted); }
	:global(.description) { color: var(--color-text-muted); font-size: 0.9rem; margin: 0 0 0.5rem; }
	:global(.entity-count) { font-size: 0.8rem; color: var(--color-text-muted); }
	:global(.plugin-controls) {
		display: flex; align-items: center; gap: 1rem; flex-wrap: wrap;
		padding: 0.75rem 1.25rem; border-top: 1px solid var(--color-border);
		margin-top: auto;
	}
	:global(.control-row) { display: flex; align-items: center; gap: 0.4rem; cursor: pointer; }
	:global(.control-label) { font-size: 0.8rem; color: var(--color-text); }
	:global(.uninstall-btn) {
		margin-left: auto; font-size: 0.8rem; color: var(--color-danger);
		padding: 0.25rem 0.5rem; border-radius: 4px;
	}
	:global(.uninstall-btn:hover) { background: rgba(220, 38, 38, 0.1); }
</style>
