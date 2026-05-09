<script lang="ts">
	import { Box, Text, Button } from '@plures/design-dojo-npm';
import Input from './Input.svelte';
import Select from './Select.svelte';

	type SettingInputType = 'toggle' | 'select' | 'text' | 'number' | 'password' | 'color';

	interface SettingDefinition {
		key: string;
		type: SettingInputType;
		label: string;
		description?: string;
		default: unknown;
		options?: { value: string; label: string }[];
		group?: string;
	}

	interface Props {
		groupName: string;
		settings: SettingDefinition[];
		getValue: (key: string) => unknown;
		setValue: (key: string, value: unknown) => void;
	}

	let { groupName, settings, getValue, setValue }: Props = $props();

	function getVal(key: string, def: unknown): string {
		const v = getValue(key);
		return v !== undefined && v !== null ? String(v) : String(def ?? '');
	}

	function handleChange(key: string, value: string, type: SettingInputType) {
		if (type === 'number') setValue(key, Number(value));
		else if (type === 'toggle') setValue(key, value === 'true');
		else setValue(key, value);
	}
</script>

<Box gap="12px" padding={4} class="settings-group">
	<Text as="p" weight="600" size="0.95rem">{groupName}</Text>

	{#each settings as setting}
		<Box gap="4px">
			{#if setting.type === 'toggle'}
				<Box direction="row" align="center" gap="8px">
					<input
						type="checkbox"
						checked={getVal(setting.key, setting.default) === 'true'}
						onchange={(e: Event) => handleChange(setting.key, String((e.target as HTMLInputElement).checked), 'toggle')}
					/>
					<Text size="0.85rem">{setting.label}</Text>
				</Box>
			{:else if setting.type === 'select' && setting.options}
				<Select
					label={setting.label}
					options={setting.options}
					value={String(getVal(setting.key, setting.default))}
					onchange={(e: Event) => handleChange(setting.key, (e.target as HTMLSelectElement).value, 'select')}
				/>
			{:else}
				<Input
					type={setting.type === 'password' ? 'password' : setting.type === 'number' ? 'number' : 'text'}
					label={setting.label}
					value={String(getVal(setting.key, setting.default))}
					onchange={(e: Event) => handleChange(setting.key, (e.target as HTMLInputElement).value, setting.type)}
				/>
			{/if}
			{#if setting.description}
				<Text size="0.75rem" color="var(--color-text-muted)">{setting.description}</Text>
			{/if}
		</Box>
	{/each}
</Box>
