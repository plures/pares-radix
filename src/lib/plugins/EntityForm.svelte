<script lang="ts">
	import { createEntity, updateEntity } from '$lib/plugins/plugin-api.js';
	import type { FieldInfo } from '$lib/plugins/plugin-api.js';

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
	let values = $state<Record<string, unknown>>({});

	$effect(() => {
		const v: Record<string, unknown> = {};
		for (const field of fields) {
			if (field.name.startsWith('_')) continue;
			v[field.name] = initialValues?.[field.name] ?? '';
		}
		values = v;
	});

	let saving = $state(false);
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

	let editableFields = $derived(fields.filter((f) => !f.name.startsWith('_')));
</script>

<form class="entity-form" onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
	<h3>{entityId ? 'Edit' : 'Create'} {entityType}</h3>

	{#each editableFields as field}
		<label class="form-field">
			<span class="label">
				{field.name}
				{#if field.required}<span class="required">*</span>{/if}
			</span>

			{#if field.field_type === 'Boolean'}
				<input type="checkbox" checked={!!values[field.name]} onchange={(e) => { values[field.name] = (e.target as HTMLInputElement).checked; }} />
			{:else if isEnum(field.field_type)}
				<select bind:value={values[field.name]}>
					<option value="">— Select —</option>
					{#each isEnum(field.field_type)! as opt}
						<option value={opt}>{opt}</option>
					{/each}
				</select>
			{:else if field.field_type === 'Currency'}
				<div class="currency-input">
					<span class="prefix">$</span>
					<input type="number" step="0.01" bind:value={values[field.name]} required={field.required} />
				</div>
			{:else}
				<input
					type={inputType(field.field_type)}
					bind:value={values[field.name]}
					required={field.required}
					placeholder={field.description ?? ''}
				/>
			{/if}
		</label>
	{/each}

	{#if error}
		<p class="error">{error}</p>
	{/if}

	<div class="form-actions">
		<button type="button" class="cancel-btn" onclick={onCancel}>Cancel</button>
		<button type="submit" class="save-btn" disabled={saving}>
			{saving ? 'Saving…' : entityId ? 'Update' : 'Create'}
		</button>
	</div>
</form>

<style>
	.entity-form { padding: 1.25rem; background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; }
	h3 { margin: 0 0 1rem; }
	.form-field { display: block; margin-bottom: 0.75rem; }
	.label { display: block; font-size: 0.85rem; font-weight: 500; margin-bottom: 0.25rem; }
	.required { color: var(--color-danger); }
	input, select {
		width: 100%; padding: 0.5rem 0.75rem; border-radius: 6px;
		border: 1px solid var(--color-border); background: var(--color-bg);
		color: var(--color-text); font-size: 0.9rem;
	}
	input[type="checkbox"] { width: auto; }
	.currency-input { display: flex; align-items: center; gap: 0.25rem; }
	.prefix { font-weight: 600; color: var(--color-text-muted); }
	.currency-input input { flex: 1; }
	.error { color: var(--color-danger); font-size: 0.85rem; }
	.form-actions { display: flex; gap: 0.75rem; justify-content: flex-end; margin-top: 1rem; }
	.cancel-btn {
		padding: 0.5rem 1rem; border-radius: 6px; cursor: pointer;
		border: 1px solid var(--color-border); background: transparent; color: var(--color-text);
	}
	.save-btn {
		padding: 0.5rem 1rem; border-radius: 6px; cursor: pointer; border: none;
		background: var(--color-accent); color: #fff; font-weight: 500;
	}
	.save-btn:disabled { opacity: 0.6; cursor: not-allowed; }
</style>
