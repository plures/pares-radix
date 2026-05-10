<script lang="ts">
	import { Box, Heading, Text, SettingsPanel, Dialog, Button } from '@plures/design-dojo';
	import {
		getAllSettings,
		exportAllPluginData,
		importAllPluginData,
		getActivePluginManifests
	} from '$lib/platform/plugin-loader.js';
	import { createExport, validateImport } from '$lib/platform/data-transfer.js';
	import { settingsAPI, clearAllSettings, exportSettings, importSettings } from '$lib/stores/settings.js';
	import { theme } from '$lib/stores/theme.js';
	import { onboarding } from '$lib/stores/onboarding.js';
	import { browser } from '$app/environment';
	import type { PluginSetting } from '$lib/types/plugin.js';

	// eslint-disable-next-line plures/no-raw-stores
	let allSettings = $derived(getAllSettings());

	let grouped = $derived.by(() => {
		const groups = new Map<string, PluginSetting[]>();
		for (const s of allSettings) {
			const ns = s.group ?? s.key.split('.')[0] ?? 'General';
			if (!groups.has(ns)) groups.set(ns, []);
			groups.get(ns)!.push(s);
		}
		return groups;
	});

	// eslint-disable-next-line plures/no-raw-stores
	let showClearConfirm = $state(false);
	// eslint-disable-next-line plures/no-raw-stores
	let exporting = $state(false);
	// eslint-disable-next-line plures/no-raw-stores
	let importing = $state(false);
	// eslint-disable-next-line plures/no-raw-stores
	let importProgress = $state({ done: 0, total: 0, current: '' });
	// eslint-disable-next-line plures/no-raw-stores
	let importError = $state<string | null>(null);

	// Platform settings rendered inline in the Platform SettingsGroup
	// eslint-disable-next-line plures/no-raw-stores
	let platformSettings: PluginSetting[] = $derived([
		{
			key: 'radix.theme',
			type: 'select',
			label: 'Theme',
			description: 'Application color scheme',
			default: theme.value,
			options: [
				{ value: 'dark', label: 'Dark' },
				{ value: 'light', label: 'Light' }
			]
		},
		{
			key: 'radix.llm.provider',
			type: 'select',
			label: 'LLM Provider',
			description: 'Language model provider for inference',
			default: '',
			options: [
				{ value: '', label: 'None' },
				{ value: 'openai', label: 'OpenAI' },
				{ value: 'anthropic', label: 'Anthropic' },
				{ value: 'copilot', label: 'GitHub Copilot' },
				{ value: 'ollama', label: 'Ollama (local)' }
			]
		},
		{
			key: 'radix.llm.apiKey',
			type: 'password',
			label: 'LLM API Key',
			description: 'API key or token for the selected provider (not required for Ollama)',
			default: ''
		},
		{
			key: 'radix.llm.model',
			type: 'text',
			label: 'Model',
			description: 'Model name to use (leave blank for provider default)',
			default: ''
		},
		{
			key: 'radix.llm.ollamaUrl',
			type: 'text',
			label: 'Ollama URL',
			description: 'Base URL for the local Ollama server',
			default: 'http://localhost:11434'
		},
		{
			key: 'radix.llm.tokenBudget',
			type: 'number',
			label: 'Session Token Budget',
			description: 'Maximum tokens to spend per session (0 = unlimited)',
			default: 50000
		}
	]);

	// Keep the theme store in sync when the platform theme setting changes.
	$effect(() => {
		const unsubscribe = settingsAPI.subscribe('radix.theme', (value) => {
			if (value === 'light' || value === 'dark') {
				theme.value = value;
			}
		});
		return unsubscribe;
	});

	async function exportData() {
		if (!browser || exporting) return;
		exporting = true;
		try {
			const pluginData = await exportAllPluginData();
			const activePlugins = getActivePluginManifests();
			const payload = createExport(exportSettings(), pluginData, activePlugins);
			const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
			const url = URL.createObjectURL(blob);
			const a = document.createElement('a');
			a.href = url;
			a.download = `radix-export-${new Date().toISOString().slice(0, 10)}.json`;
			a.click();
			URL.revokeObjectURL(url);
		} finally {
			exporting = false;
		}
	}

	function importData() {
		if (!browser || importing) return;
		importError = null;
		const input = document.createElement('input');
		input.type = 'file';
		input.accept = '.json';
		input.onchange = async () => {
			const file = input.files?.[0];
			if (!file) return;
			importing = true;
			importProgress = { done: 0, total: 0, current: '' };
			try {
				const text = await file.text();
				const raw: unknown = JSON.parse(text);

				if (!validateImport(raw)) {
					importError =
						'Invalid or incompatible export file. Please use a file exported from this application.';
					return;
				}

				if (raw.settings) importSettings(raw.settings);

				if (raw.plugins) {
					await importAllPluginData(raw.plugins, (done, total, pluginId) => {
						importProgress = { done, total, current: pluginId };
					});
				}

				window.location.reload();
			} catch (err) {
				// eslint-disable-next-line plures/no-manual-logging
				console.error('[radix] Import failed:', err);
				importError = 'Failed to read the file. Make sure it is a valid JSON export.';
			} finally {
				importing = false;
			}
		};
		input.click();
	}

	function clearAllData() {
		if (!browser) return;
		clearAllSettings();
		onboarding.reset();
		showClearConfirm = false;
		window.location.reload();
	}
</script>

<svelte:head>
	<title>Radix — Settings</title>
</svelte:head>

<Heading level={1} class="page-title">Settings</Heading>

<SettingsPanel groupName="Platform" settings={platformSettings} getValue={settingsAPI.get} setValue={settingsAPI.set} />
<Text as="p" class="api-key-note">⚠️ API keys are stored locally on this device. Do not use this on shared computers.</Text>

{#each [...grouped.entries()] as [name, pluginSettings]}
	<Box class="group-spacer">
		<SettingsPanel groupName={name} settings={pluginSettings} getValue={settingsAPI.get} setValue={settingsAPI.set} />
	</Box>
{/each}

<Box class="data-section">
	<Heading level={2} class="section-title">Data Management</Heading>
	<Box class="data-actions">
		<Button variant="secondary" onclick={exportData} disabled={exporting || importing}>
			{exporting ? '⏳ Exporting…' : '📦 Export All Data'}
		</Button>
		<Button variant="secondary" onclick={importData} disabled={exporting || importing}>
			{importing ? '⏳ Importing…' : '📥 Import Data'}
		</Button>
		<Button variant="secondary" onclick={() => showClearConfirm = true} disabled={exporting || importing}>🗑️ Clear All Data</Button>
	</Box>

	{#if importing && importProgress.total > 0}
		<Box class="import-progress" role="status" aria-live="polite">
			<Box class="progress-bar-track">
				<Box class="progress-bar-fill" style="width: {Math.round((importProgress.done / importProgress.total) * 100)}%">
					<Text as="span" class="sr-only">Import progress</Text>
				</Box>
			</Box>
			<Text as="p" class="progress-label">
				Importing plugin data… {importProgress.done}/{importProgress.total}
				{#if importProgress.current}
					<Text as="span" class="progress-plugin">({importProgress.current})</Text>
				{/if}
			</Text>
		</Box>
	{/if}

	{#if importError}
		<Text as="p" class="import-error" role="alert">{importError}</Text>
	{/if}
</Box>

<Dialog
	open={showClearConfirm}
	title="Clear All Data"
	message="This will permanently delete all settings, onboarding progress, and plugin data. This cannot be undone."
	confirmLabel="Clear Everything"
	onConfirm={clearAllData}
	onCancel={() => showClearConfirm = false}
/>

<style>
	:global(.page-title) { margin: 0 0 24px; }
	:global(.section-title) { font-size: 1.1rem; margin: 0 0 12px; color: var(--color-text); }
	:global(.group-spacer) { margin-top: 16px; }

	:global(.data-section) {
		margin-top: 32px;
		padding: 16px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
	}

	:global(.data-actions) { display: flex; gap: 8px; flex-wrap: wrap; }

	:global(.import-progress) {
		margin-top: 12px;
	}

	:global(.progress-bar-track) {
		height: 6px;
		border-radius: 3px;
		background: var(--color-border);
		overflow: hidden;
	}

	:global(.progress-bar-fill) {
		height: 100%;
		border-radius: 3px;
		background: var(--color-accent, #6366f1);
		transition: width 0.2s ease;
	}

	:global(.progress-label) {
		margin: 6px 0 0;
		font-size: 0.78rem;
		color: var(--color-text-muted);
	}

	:global(.progress-plugin) {
		opacity: 0.75;
	}

	:global(.import-error) {
		margin-top: 10px;
		padding: 8px 12px;
		border-radius: 6px;
		background: color-mix(in srgb, var(--color-error, #ef4444) 12%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-error, #ef4444) 40%, transparent);
		color: var(--color-error, #ef4444);
		font-size: 0.82rem;
	}

	:global(.api-key-note) {
		margin: 6px 0 0;
		font-size: 0.78rem;
		color: var(--color-text-muted);
	}
</style>
