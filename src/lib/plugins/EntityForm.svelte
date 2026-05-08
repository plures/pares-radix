<script lang="ts">
	import { createEntity, updateEntity } from '$lib/plugins/plugin-api.js';
	import type { FieldInfo } from '$lib/plugins/plugin-api.js';
	import { Box, Button, Heading, Input, Select, Text } from '@plures/design-dojo';

	interface Props {
		pluginName: string;
		entityType: string;
		fields: FieldInfo[];
		entityId?: string | null;
		initialValues?: Record<string, unknown>;
		onSaved: () => void;
		onCancel: () => void;
	}

	let { pluginName, entityType, fields, entityId = null, initialValues, onSaved, onCancel }: Props = $props();

	// Form values — seed from initialValues if editing
	// eslint-disable-next-line plures/no-raw-stores
	let values = $state<Record<string, string | boolean>>({});

	$effect(() => {
		const v: Record<string, string | boolean> = {};
		for (const field of fields) {
			if (field.name.startsWith('_')) continue;
			if (field.field_type === 'Boolean') {
				v[field.name] = Boolean(initialValues?.[field.name]);
			} else {
				v[field.name] = String(initialValues?.[field.name] ?? '');
			}
		}
		values = v;
	});

	// eslint-disable-next-line plures/no-raw-stores
	let saving = $state(false);
	// eslint-disable-next-line plures/no-raw-stores
	let error = $state<string | null>(null);

	async function handleSubmit() {
		saving = true;
		error = null;
		try {
			if (entityId) {
				await updateEntity(entityId, values);
			} else {
				await createEntity(pluginName, entityType, values);
			}
			onSaved();
		} catch (e) {
			error = String(e);
		} finally {
			saving = false;
		}
	}

	function inputType(ft: string): string {
		if (ft === 'Number' || ft === 'Currency') return 'number';
		if (ft === 'Date') return 'date';
		return 'text';
	}

	function isEnum(ft: string): string[] | null {
		const m = ft.match(/^Enum\((.+)\)$/);
		if (!m) return null;
		return m[1].split(',');
	}

	// eslint-disable-next-line plures/no-raw-stores
	let editableFields = $derived(fields.filter((f) => !f.name.startsWith('_')));
</script>

<Box as="form" class="entity-form" onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
	<Heading level={3}>{entityId ? 'Edit' : 'Create'} {entityType}</Heading>

	{#each editableFields as field}
		<Box class="form-field">
			<Text as="span" class="label">
				{field.name}
				{#if field.required}<Text as="span" class="required">*</Text>{/if}
			</Text>

			{#if field.field_type === 'Boolean'}
				<Input
					type="checkbox"
					checked={Boolean(values[field.name])}
					onchange={(e) => { values[field.name] = (e.target as HTMLInputElement).checked; }}
				/>
			{:else if isEnum(field.field_type)}
				<Select
					value={String(values[field.name] ?? '')}
					required={field.required}
					placeholder="— Select —"
					options={isEnum(field.field_type)!.map((opt) => ({ label: opt, value: opt }))}
					onchange={(e) => { values[field.name] = (e.target as HTMLSelectElement).value; }}
				/>
			{:else if field.field_type === 'Currency'}
				<Box class="currency-input" direction="row" align="center" gap="0.25rem">
					<Text as="span" class="prefix">$</Text>
					<Input type="number" value={String(values[field.name] ?? '')} required={field.required} oninput={(e) => { values[field.name] = (e.target as HTMLInputElement).value; }} />
				</Box>
			{:else}
				<Input
					type={inputType(field.field_type)}
					value={String(values[field.name] ?? '')}
					required={field.required}
					placeholder={field.description ?? ''}
					oninput={(e) => { values[field.name] = (e.target as HTMLInputElement).value; }}
				/>
			{/if}
		</Box>
	{/each}

	{#if error}
		<Text as="p" class="error">{error}</Text>
	{/if}

	<Box class="form-actions" direction="row" justify="flex-end" gap="0.75rem">
		<Button
			variant="secondary"
			onclick={(e) => { e.preventDefault(); onCancel(); }}
		>Cancel</Button>
		<Button
			variant="primary"
			disabled={saving}
			onclick={(e) => { e.preventDefault(); handleSubmit(); }}
		>
			{saving ? 'Saving…' : entityId ? 'Update' : 'Create'}
		</Button>
	</Box>
</Box>

<style>
	:global(.entity-form) { padding: 1.25rem; background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; }
	:global(.entity-form h3) { margin: 0 0 1rem; }
	:global(.form-field) { display: block; margin-bottom: 0.75rem; }
	:global(.label) { display: block; font-size: 0.85rem; font-weight: 500; margin-bottom: 0.25rem; }
	:global(.required) { color: var(--color-danger); }
	:global(.currency-input) { display: flex; align-items: center; gap: 0.25rem; }
	:global(.prefix) { font-weight: 600; color: var(--color-text-muted); }
	:global(.error) { color: var(--color-danger); font-size: 0.85rem; }
	:global(.form-actions) { display: flex; gap: 0.75rem; justify-content: flex-end; margin-top: 1rem; }
</style>
