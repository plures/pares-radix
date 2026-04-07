<script lang="ts">
	import type { SettingsPanelProps, SettingDefinition } from './types.js';

	let { groupName, settings, getValue, setValue }: SettingsPanelProps = $props();

	// Reactive local values — initialised from getValue and updated on change
	// so that controls reflect persisted state without a page reload.
	let values: Record<string, unknown> = $state({});

	$effect(() => {
		for (const s of settings) {
			const stored = getValue(s.key);
			values[s.key] = stored !== undefined ? stored : s.default;
		}
	});

	function getVal(setting: SettingDefinition): unknown {
		if (setting.key in values) return values[setting.key];
		const stored = getValue(setting.key);
		return stored !== undefined ? stored : setting.default;
	}

	function setVal(setting: SettingDefinition, value: unknown): void {
		values[setting.key] = value;
		setValue(setting.key, value);
	}
</script>

<fieldset class="settings-group">
	<legend>{groupName}</legend>

	{#each settings as setting}
		<div class="setting">
			<div class="setting-info">
				<label class="setting-label" for={setting.key}>{setting.label}</label>
				{#if setting.description}
					<span class="setting-desc">{setting.description}</span>
				{/if}
			</div>

			<div class="setting-control">
				{#if setting.type === 'toggle'}
					<input
						id={setting.key}
						type="checkbox"
						class="toggle"
						checked={getVal(setting) as boolean}
						onchange={(e) => setVal(setting, (e.target as HTMLInputElement).checked)}
					/>
				{:else if setting.type === 'select'}
					<select
						id={setting.key}
						class="select"
						value={getVal(setting) as string}
						onchange={(e) => setVal(setting, (e.target as HTMLSelectElement).value)}
					>
						{#each setting.options ?? [] as opt}
							<option value={opt.value}>{opt.label}</option>
						{/each}
					</select>
				{:else if setting.type === 'number'}
					<input
						id={setting.key}
						class="input"
						type="number"
						value={String(getVal(setting))}
						onchange={(e) => setVal(setting, Number((e.target as HTMLInputElement).value))}
					/>
				{:else if setting.type === 'password'}
					<input
						id={setting.key}
						class="input"
						type="password"
						value={getVal(setting) as string}
						onchange={(e) => setVal(setting, (e.target as HTMLInputElement).value)}
					/>
				{:else if setting.type === 'color'}
					<input
						id={setting.key}
						class="color-input"
						type="color"
						value={getVal(setting) as string}
						onchange={(e) => setVal(setting, (e.target as HTMLInputElement).value)}
					/>
				{:else}
					<input
						id={setting.key}
						class="input"
						type="text"
						value={getVal(setting) as string}
						onchange={(e) => setVal(setting, (e.target as HTMLInputElement).value)}
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
		cursor: pointer;
	}

	.setting-desc {
		display: block;
		font-size: 0.8rem;
		color: var(--color-text-muted);
		margin-top: 2px;
	}

	.setting-control { flex-shrink: 0; }

	.input, .select {
		padding: 6px 10px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-bg);
		color: var(--color-text);
		font-size: 0.875rem;
	}

	.select { min-width: 120px; }

	.toggle {
		width: 1.1rem;
		height: 1.1rem;
		accent-color: var(--color-accent);
		cursor: pointer;
	}

	.color-input {
		width: 2.5rem;
		height: 2rem;
		padding: 2px 3px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-bg);
		cursor: pointer;
	}
</style>
