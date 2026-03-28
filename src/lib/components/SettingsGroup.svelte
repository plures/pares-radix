<script lang="ts">
	import { Input, Select, Toggle } from '@plures/design-dojo';
	import type { PluginSetting } from '$lib/types/plugin.js';
	import { browser } from '$app/environment';

	interface Props {
		groupName: string;
		settings: PluginSetting[];
	}

	let { groupName, settings }: Props = $props();

	function getValue(setting: PluginSetting): unknown {
		if (!browser) return setting.default;
		const stored = localStorage.getItem(`radix-setting:${setting.key}`);
		return stored !== null ? JSON.parse(stored) : setting.default;
	}

	function setValue(setting: PluginSetting, value: unknown) {
		if (browser) {
			localStorage.setItem(`radix-setting:${setting.key}`, JSON.stringify(value));
		}
	}
</script>

<fieldset class="settings-group">
	<legend>{groupName}</legend>

	{#each settings as setting}
		<div class="setting">
			<div class="setting-info">
				<span class="setting-label">{setting.label}</span>
				{#if setting.description}
					<span class="setting-desc">{setting.description}</span>
				{/if}
			</div>

			<div class="setting-control">
				{#if setting.type === 'toggle'}
					<Toggle
						checked={getValue(setting) as boolean}
						onchange={(e) => setValue(setting, (e.target as HTMLInputElement).checked)}
					/>
				{:else if setting.type === 'select'}
					<Select
						id={setting.key}
						value={getValue(setting) as string}
						onchange={(e) => setValue(setting, (e.target as HTMLSelectElement).value)}
					>
						{#each setting.options ?? [] as opt}
							<option value={opt.value}>{opt.label}</option>
						{/each}
					</Select>
				{:else if setting.type === 'number'}
					<Input
						id={setting.key}
						type="number"
						value={String(getValue(setting))}
						onchange={(e) => setValue(setting, Number((e.target as HTMLInputElement).value))}
					/>
				{:else if setting.type === 'password'}
					<Input
						id={setting.key}
						type="password"
						value={getValue(setting) as string}
						onchange={(e) => setValue(setting, (e.target as HTMLInputElement).value)}
					/>
				{:else}
					<Input
						id={setting.key}
						type="text"
						value={getValue(setting) as string}
						onchange={(e) => setValue(setting, (e.target as HTMLInputElement).value)}
					/>
				{/if}
			</div>
		</div>
	{/each}
</fieldset>

<style>
	.settings-group {
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 16px;
		margin: 0;
	}

	legend {
		font-weight: 600;
		font-size: 0.95rem;
		color: var(--color-text);
		padding: 0 8px;
	}

	.setting {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		padding: 12px 0;
		border-bottom: 1px solid var(--color-border);
		gap: 16px;
	}

	.setting:last-child { border-bottom: none; }
	.setting-info { flex: 1; }

	.setting-label {
		display: block;
		font-size: 0.9rem;
		color: var(--color-text);
	}

	.setting-desc {
		display: block;
		font-size: 0.8rem;
		color: var(--color-text-muted);
		margin-top: 2px;
	}

	.setting-control { flex-shrink: 0; }
</style>
