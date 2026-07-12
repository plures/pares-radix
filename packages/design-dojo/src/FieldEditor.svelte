<script lang="ts">
	import Input from './Input.svelte';
	import Select from './Select.svelte';
	import type {
		FieldEditorProps,
		SchemaField,
		SchemaFieldType,
		SelectOption
	} from './types-local.js';

	let {
		field,
		submitLabel = 'Save field',
		cancelLabel = 'Cancel',
		disabled = false,
		existingNames = [],
		onchange,
		onsubmit,
		oncancel,
		class: className = ''
	}: FieldEditorProps = $props();

	const TYPE_OPTIONS: SelectOption[] = [
		{ value: 'string', label: 'Text' },
		{ value: 'number', label: 'Number' },
		{ value: 'boolean', label: 'Boolean' },
		{ value: 'datetime', label: 'Date/time' },
		{ value: 'select', label: 'Select (options)' }
	];

	// Local editable draft, seeded from the `field` prop (create = blank).
	let name = $state('');
	let type = $state<SchemaFieldType>('string');
	let description = $state('');
	let required = $state(false);
	let optionsText = $state('');
	let submitted = $state(false);

	$effect(() => {
		name = field?.name ?? '';
		type = field?.type ?? 'string';
		description = field?.description ?? '';
		required = field?.required ?? false;
		optionsText = (field?.options ?? [])
			.map((o) => (o.value === o.label ? o.value : `${o.value}=${o.label}`))
			.join('\n');
	});

	function parseOptions(text: string): SelectOption[] {
		return text
			.split(/\r?\n/)
			.map((l) => l.trim())
			.filter(Boolean)
			.map((l) => {
				const eq = l.indexOf('=');
				if (eq === -1) return { value: l, label: l };
				return { value: l.slice(0, eq).trim(), label: l.slice(eq + 1).trim() };
			});
	}

	const draft = $derived.by<SchemaField>(() => {
		const f: SchemaField = { name: name.trim(), type, required };
		const d = description.trim();
		if (d) f.description = d;
		if (type === 'select') f.options = parseOptions(optionsText);
		return f;
	});

	const nameError = $derived.by<string | undefined>(() => {
		const n = name.trim();
		if (!n) return 'Field name is required';
		if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(n)) {
			return 'Use letters, digits, underscore; cannot start with a digit';
		}
		if (existingNames.includes(n) && n !== field?.name) {
			return `Field "${n}" already exists`;
		}
		return undefined;
	});

	const optionsError = $derived.by<string | undefined>(() => {
		if (type !== 'select') return undefined;
		return parseOptions(optionsText).length === 0
			? 'At least one option is required for a select field'
			: undefined;
	});

	const valid = $derived(!nameError && !optionsError);

	// Live change notification (host may preview without committing).
	$effect(() => {
		if (valid) onchange?.(draft);
	});

	function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		submitted = true;
		if (!valid) return;
		onsubmit?.(draft);
	}
</script>

<form class="field-editor {className}" onsubmit={handleSubmit} novalidate>
	<Input
		name="field-name"
		label="Field name"
		required
		{disabled}
		error={submitted ? nameError : undefined}
		bind:value={name}
	/>

	<Select
		name="field-type"
		label="Type"
		options={TYPE_OPTIONS}
		{disabled}
		bind:value={
			() => type,
			(v) => (type = v as SchemaFieldType)
		}
	/>

	<Input
		name="field-description"
		label="Description"
		{disabled}
		bind:value={description}
	/>

	{#if type === 'select'}
		<div class="field">
			<label class="fe-label" for="field-options">Options (one per line, <code>value=Label</code>)</label>
			<textarea
				id="field-options"
				class="fe-textarea"
				class:has-error={submitted && !!optionsError}
				{disabled}
				rows="4"
				bind:value={optionsText}
			></textarea>
			{#if submitted && optionsError}<span class="fe-error">{optionsError}</span>{/if}
		</div>
	{/if}

	<label class="checkbox-label">
		<input type="checkbox" {disabled} bind:checked={required} />
		<span>Required</span>
	</label>

	<div class="actions">
		{#if oncancel}
			<button type="button" class="btn btn-ghost" {disabled} onclick={() => oncancel?.()}>
				{cancelLabel}
			</button>
		{/if}
		<button type="submit" class="btn btn-primary" disabled={disabled}>{submitLabel}</button>
	</div>
</form>

<style>
	.field-editor {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.fe-label {
		font-size: 0.8rem;
		font-weight: 500;
		color: var(--color-text-muted, #888);
	}
	.fe-label code {
		font-size: 0.75rem;
	}
	.fe-textarea {
		padding: 7px 10px;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-text);
		font-size: 0.85rem;
		font-family: inherit;
		outline: none;
		resize: vertical;
		transition: border-color 0.12s;
	}
	.fe-textarea:focus {
		border-color: var(--color-accent, #6366f1);
	}
	.fe-textarea.has-error {
		border-color: var(--color-danger, #ef4444);
	}
	.fe-error {
		font-size: 0.75rem;
		color: var(--color-danger, #ef4444);
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
