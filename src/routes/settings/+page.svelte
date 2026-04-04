<script lang="ts">
	import SettingsGroup from '$lib/components/SettingsGroup.svelte';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
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

	let showClearConfirm = $state(false);
	let exporting = $state(false);
	let importing = $state(false);
	let importProgress = $state({ done: 0, total: 0, current: '' });
	let importError = $state<string | null>(null);

	// Platform settings rendered inline in the Platform SettingsGroup
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

<h1>Settings</h1>

<SettingsGroup groupName="Platform" settings={platformSettings} />
<p class="api-key-note">⚠️ API keys are stored locally on this device. Do not use this on shared computers.</p>

{#each [...grouped.entries()] as [name, pluginSettings]}
	<div class="group-spacer">
		<SettingsGroup groupName={name} settings={pluginSettings} />
	</div>
{/each}

<div class="data-section">
	<h2>Data Management</h2>
	<div class="data-actions">
		<button class="btn secondary" onclick={exportData} disabled={exporting || importing}>
			{exporting ? '⏳ Exporting…' : '📦 Export All Data'}
		</button>
		<button class="btn secondary" onclick={importData} disabled={exporting || importing}>
			{importing ? '⏳ Importing…' : '📥 Import Data'}
		</button>
		<button class="btn secondary" onclick={() => showClearConfirm = true} disabled={exporting || importing}>🗑️ Clear All Data</button>
	</div>

	{#if importing && importProgress.total > 0}
		<div class="import-progress" role="status" aria-live="polite">
			<div class="progress-bar-track">
				<div
					class="progress-bar-fill"
					style="width: {Math.round((importProgress.done / importProgress.total) * 100)}%"
				></div>
			</div>
			<p class="progress-label">
				Importing plugin data… {importProgress.done}/{importProgress.total}
				{#if importProgress.current}
					<span class="progress-plugin">({importProgress.current})</span>
				{/if}
			</p>
		</div>
	{/if}

	{#if importError}
		<p class="import-error" role="alert">{importError}</p>
	{/if}
</div>

<ConfirmDialog
	open={showClearConfirm}
	title="Clear All Data"
	message="This will permanently delete all settings, onboarding progress, and plugin data. This cannot be undone."
	confirmLabel="Clear Everything"
	onConfirm={clearAllData}
	onCancel={() => showClearConfirm = false}
/>

<style>
	h1 { margin: 0 0 24px; }
	h2 { font-size: 1.1rem; margin: 0 0 12px; color: var(--color-text); }
	.group-spacer { margin-top: 16px; }

	.data-section {
		margin-top: 32px;
		padding: 16px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
	}

	.data-actions { display: flex; gap: 8px; flex-wrap: wrap; }

	.btn {
		padding: 7px 14px;
		border-radius: 6px;
		font-size: 0.85rem;
		cursor: pointer;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-text);
		font-weight: 500;
		transition: background 0.12s;
	}

	.btn:hover { background: var(--color-hover); }
	.btn.secondary { background: var(--color-surface); }
	.btn:disabled { opacity: 0.55; cursor: not-allowed; }

	.import-progress {
		margin-top: 12px;
	}

	.progress-bar-track {
		height: 6px;
		border-radius: 3px;
		background: var(--color-border);
		overflow: hidden;
	}

	.progress-bar-fill {
		height: 100%;
		border-radius: 3px;
		background: var(--color-accent, #6366f1);
		transition: width 0.2s ease;
	}

	.progress-label {
		margin: 6px 0 0;
		font-size: 0.78rem;
		color: var(--color-text-muted);
	}

	.progress-plugin {
		opacity: 0.75;
	}

	.import-error {
		margin-top: 10px;
		padding: 8px 12px;
		border-radius: 6px;
		background: color-mix(in srgb, var(--color-error, #ef4444) 12%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-error, #ef4444) 40%, transparent);
		color: var(--color-error, #ef4444);
		font-size: 0.82rem;
	}

	.api-key-note {
		margin: 6px 0 0;
		font-size: 0.78rem;
		color: var(--color-text-muted);
	}
</style>

