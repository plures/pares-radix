<script lang="ts">
	import SettingsGroup from '$lib/components/SettingsGroup.svelte';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import { getAllSettings, exportAllPluginData, importAllPluginData } from '$lib/platform/plugin-loader.js';
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
				{ value: 'ollama', label: 'Ollama (local)' }
			]
		},
		{
			key: 'radix.llm.apiKey',
			type: 'password',
			label: 'LLM API Key',
			description: 'API key for the selected provider',
			default: ''
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
		if (!browser) return;
		const pluginData = await exportAllPluginData();
		const blob = new Blob(
			[JSON.stringify({ settings: exportSettings(), plugins: pluginData }, null, 2)],
			{ type: 'application/json' }
		);
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = 'radix-export.json';
		a.click();
		URL.revokeObjectURL(url);
	}

	function importData() {
		if (!browser) return;
		const input = document.createElement('input');
		input.type = 'file';
		input.accept = '.json';
		input.onchange = async () => {
			const file = input.files?.[0];
			if (!file) return;
			const text = await file.text();
			const data = JSON.parse(text) as {
				settings?: Record<string, unknown>;
				plugins?: Record<string, unknown>;
			};
			if (data.settings) importSettings(data.settings);
			if (data.plugins) await importAllPluginData(data.plugins);
			window.location.reload();
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
		<button class="btn secondary" onclick={exportData}>📦 Export All Data</button>
		<button class="btn secondary" onclick={importData}>📥 Import Data</button>
		<button class="btn secondary" onclick={() => showClearConfirm = true}>🗑️ Clear All Data</button>
	</div>
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

	.api-key-note {
		margin: 6px 0 0;
		font-size: 0.78rem;
		color: var(--color-text-muted);
	}
</style>

