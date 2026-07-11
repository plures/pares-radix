<script lang="ts">
	import Input from './Input.svelte';
	import Select from './Select.svelte';
	import type {
		DataRow,
		SchemaField,
		SchemaFormErrors,
		SchemaFormProps
	} from './types-local.js';

	let {
		schema,
		value,
		validate,
		submitLabel = 'Save',
		cancelLabel = 'Cancel',
		disabled = false,
		onsubmit,
		oncancel,
		class: className = ''
	}: SchemaFormProps = $props();

	const fields = $derived(schema.fields.filter((f) => !f.hidden));

	function fieldLabel(field: SchemaField): string {
		if (field.label) return field.label;
		return field.name
			.replace(/[_-]+/g, ' ')
			.replace(/([a-z])([A-Z])/g, '$1 $2')
			.replace(/^\w/, (c) => c.toUpperCase());
	}

	function initialFor(field: SchemaField): unknown {
		const seed = value?.[field.name];
		if (seed !== undefined && seed !== null) return seed;
		switch (field.type) {
			case 'boolean':
				return false;
			case 'number':
				return '';
			default:
				return '';
		}
	}

	// Local editable state, seeded from `value` (create = blank, edit = populated).
	let form = $state<Record<string, unknown>>({});
	$effect(() => {
		const next: Record<string, unknown> = {};
		for (const f of fields) next[f.name] = initialFor(f);
		form = next;
	});

	let errors = $state<SchemaFormErrors>({});
	let submitted = $state(false);

	// Convert a datetime value into a `<input type="date">`-friendly yyyy-mm-dd.
	function toDateInput(v: unknown): string {
		if (!v) return '';
		const d = v instanceof Date ? v : new Date(String(v));
		if (Number.isNaN(d.getTime())) return '';
		return d.toISOString().slice(0, 10);
	}

	function assemble(): DataRow {
		const record: DataRow = { ...(value ?? {}) };
		for (const f of fields) {
			const raw = form[f.name];
			switch (f.type) {
				case 'number':
					record[f.name] = raw === '' || raw === null || raw === undefined ? null : Number(raw);
					break;
				case 'boolean':
					record[f.name] = !!raw;
					break;
				case 'datetime':
					record[f.name] = raw ? new Date(String(raw)).toISOString() : null;
					break;
				default:
					record[f.name] = raw;
			}
		}
		return record;
	}

	function requiredErrors(record: DataRow): SchemaFormErrors {
		const errs: SchemaFormErrors = {};
		for (const f of fields) {
			if (!f.required) continue;
			const v = record[f.name];
			const empty = v === null || v === undefined || v === '';
			if (empty && f.type !== 'boolean') errs[f.name] = `${fieldLabel(f)} is required`;
		}
		return errs;
	}

	function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		submitted = true;
		const record = assemble();
		const combined: SchemaFormErrors = {
			...requiredErrors(record),
			...(validate ? validate(record) : {})
		};
		errors = combined;
		if (Object.keys(combined).length > 0) return;
		onsubmit?.(record);
	}
</script>

<form class="schema-form {className}" onsubmit={handleSubmit} novalidate>
	{#if schema.name}
		<h3 class="form-title">{schema.name}</h3>
	{/if}

	{#each fields as field (field.name)}
		<div class="field">
			{#if field.type === 'boolean'}
				<label class="checkbox-label">
					<input
						type="checkbox"
						name={field.name}
						{disabled}
						checked={!!form[field.name]}
						onchange={(e) => (form[field.name] = (e.currentTarget as HTMLInputElement).checked)}
					/>
					<span>{fieldLabel(field)}</span>
				</label>
				{#if field.description}<span class="field-desc">{field.description}</span>{/if}
			{:else if field.type === 'select'}
				<Select
					name={field.name}
					label={fieldLabel(field)}
					options={field.options ?? []}
					placeholder="Select…"
					required={field.required}
					{disabled}
					bind:value={
						() => String(form[field.name] ?? ''),
						(v) => (form[field.name] = v)
					}
				/>
				{#if field.description}<span class="field-desc">{field.description}</span>{/if}
			{:else if field.type === 'datetime'}
				<Input
					type="date"
					name={field.name}
					label={fieldLabel(field)}
					required={field.required}
					error={submitted ? errors[field.name] : undefined}
					{disabled}
					bind:value={
						() => toDateInput(form[field.name]),
						(v) => (form[field.name] = v)
					}
				/>
				{#if field.description}<span class="field-desc">{field.description}</span>{/if}
			{:else}
				<Input
					type={field.type === 'number' ? 'number' : 'text'}
					name={field.name}
					label={fieldLabel(field)}
					required={field.required}
					error={submitted ? errors[field.name] : undefined}
					{disabled}
					bind:value={
						() => (form[field.name] as string | number) ?? '',
						(v) => (form[field.name] = v)
					}
				/>
				{#if field.description}<span class="field-desc">{field.description}</span>{/if}
			{/if}
		</div>
	{/each}

	<div class="actions">
		{#if oncancel}
			<button type="button" class="btn btn-ghost" {disabled} onclick={() => oncancel?.()}>
				{cancelLabel}
			</button>
		{/if}
		<button type="submit" class="btn btn-primary" {disabled}>{submitLabel}</button>
	</div>
</form>

<style>
	.schema-form {
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	.form-title {
		margin: 0;
		font-size: 1rem;
		color: var(--color-text);
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.field-desc {
		font-size: 0.75rem;
		color: var(--color-text-muted);
	}

	.checkbox-label {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		font-size: 0.85rem;
		color: var(--color-text);
		cursor: pointer;
	}

	.checkbox-label input {
		width: 16px;
		height: 16px;
		accent-color: var(--color-accent, #6366f1);
		cursor: pointer;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 4px;
	}

	.btn {
		border-radius: 6px;
		padding: 8px 16px;
		font-size: 0.85rem;
		font-weight: 500;
		cursor: pointer;
		border: 1px solid transparent;
		transition: background 0.12s, border-color 0.12s;
	}

	.btn:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}

	.btn-primary {
		background: var(--color-accent, #6366f1);
		color: #fff;
	}

	.btn-primary:hover:not(:disabled) {
		filter: brightness(1.08);
	}

	.btn-ghost {
		background: transparent;
		border-color: var(--color-border);
		color: var(--color-text);
	}

	.btn-ghost:hover:not(:disabled) {
		background: var(--color-hover);
	}
</style>
