<script lang="ts">
	import SettingsGroup from '$lib/components/SettingsGroup.svelte';
	import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
	import { getAllSettings } from '$lib/platform/plugin-loader.js';
	import { theme } from '$lib/stores/theme.js';
	import { onboarding } from '$lib/stores/onboarding.js';
	import { browser } from '$app/environment';

	let allSettings = $derived(getAllSettings());

	let grouped = $derived(() => {
		const groups = new Map<string, typeof allSettings>();
		for (const s of allSettings) {
			const ns = s.group ?? s.key.split('.')[0] ?? 'General';
			if (!groups.has(ns)) groups.set(ns, []);
			groups.get(ns)!.push(s);
		}
		return groups;
	});

	let showClearConfirm = $state(false);

	function exportData() {
		if (!browser) return;
		const data: Record<string, string | null> = {};
		for (let i = 0; i < localStorage.length; i++) {
			const key = localStorage.key(i);
			if (key?.startsWith('radix-')) {
				data[key] = localStorage.getItem(key);
			}
		}
		const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
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
			const data = JSON.parse(text);
			for (const [key, value] of Object.entries(data)) {
				if (typeof value === 'string') localStorage.setItem(key, value);
			}
			window.location.reload();
		};
		input.click();
	}

	function clearAllData() {
		if (!browser) return;
		const keys: string[] = [];
		for (let i = 0; i < localStorage.length; i++) {
			const key = localStorage.key(i);
			if (key?.startsWith('radix-')) keys.push(key);
		}
		keys.forEach(k => localStorage.removeItem(k));
		onboarding.reset();
		showClearConfirm = false;
		window.location.reload();
	}
</script>

<svelte:head>
	<title>Radix — Settings</title>
</svelte:head>

<h1>Settings</h1>

<SettingsGroup groupName="Platform" settings={[
	{
		key: 'radix.theme',
		type: 'select',
		label: 'Theme',
		description: 'Application color scheme',
		default: theme.value,
		options: [{ value: 'dark', label: 'Dark' }, { value: 'light', label: 'Light' }]
	}
]} />

{#each [...grouped().entries()] as [name, pluginSettings]}
	<div class="group-spacer">
		<SettingsGroup groupName={name} settings={pluginSettings} />
	</div>
{/each}

<div class="data-section">
	<h2>Data Management</h2>
	<div class="data-actions">
		<button class="btn-secondary" onclick={exportData}>📦 Export All Data</button>
		<button class="btn-secondary" onclick={importData}>📥 Import Data</button>
		<button class="btn-secondary" onclick={() => showClearConfirm = true}>🗑️ Clear All Data</button>
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

	.btn-secondary {
		padding: 7px 14px;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-text);
		font-size: 0.875rem;
		cursor: pointer;
	}

	.btn-secondary:hover { background: var(--color-hover); }
</style>
