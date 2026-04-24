<script lang="ts">
	import { untrack } from 'svelte';
	import { query, emitFact } from '$lib/stores/praxis-svelte.js';
	import type { DesignSchema } from '$lib/praxis/design.js';

	interface Props {
		schema: DesignSchema;
		onSave: (definition: Record<string, unknown>) => void;
		onCancel: () => void;
	}

	let { schema, onSave, onCancel }: Props = $props();

	// Draft state
	let draft = $state<Record<string, unknown>>(untrack(() => ({ ...schema.definition })));
	let dirty = $state(false);

	// Validation from praxis
	let validation = $derived(
		query<{ valid: boolean; errors: string[] }>('design.edit.validation') ?? { valid: true, errors: [] }
	);

	// Track changes
	function updateField(key: string, value: unknown) {
		draft = { ...draft, [key]: value };
		dirty = true;
		emitFact('design.schema.draft.updated', {
			schemaId: schema.id,
			definition: draft,
		});
	}

	function handleSave() {
		if (validation.valid) {
			onSave(draft);
		}
	}

	// Determine editable fields based on schema kind
	let editableFields = $derived(() => {
		switch (schema.kind) {
			case 'rule':
				return [
					{ key: 'id', label: 'Rule ID', type: 'text' as const, required: true },
					{ key: 'description', label: 'Description', type: 'textarea' as const, required: true },
					{ key: 'trigger', label: 'Trigger Event', type: 'text' as const, required: true },
					{ key: 'emits', label: 'Emits (comma-separated)', type: 'text' as const, required: false },
				];
			case 'constraint':
				return [
					{ key: 'id', label: 'Constraint ID', type: 'text' as const, required: true },
					{ key: 'description', label: 'Description', type: 'textarea' as const, required: true },
					{ key: 'message', label: 'Violation Message', type: 'textarea' as const, required: true },
				];
			case 'fact':
				return [
					{ key: 'id', label: 'Fact ID', type: 'text' as const, required: true },
					{ key: 'description', label: 'Description', type: 'textarea' as const, required: true },
					{ key: 'persist', label: 'Persist to PluresDB', type: 'checkbox' as const, required: false },
				];
			case 'event':
				return [
					{ key: 'id', label: 'Event ID', type: 'text' as const, required: true },
					{ key: 'description', label: 'Description', type: 'textarea' as const, required: true },
					{ key: 'schema', label: 'Payload Schema', type: 'text' as const, required: false },
				];
			case 'gate':
				return [
					{ key: 'id', label: 'Gate ID', type: 'text' as const, required: true },
					{ key: 'description', label: 'Description', type: 'textarea' as const, required: true },
					{ key: 'conditions', label: 'Required Facts (comma-separated)', type: 'text' as const, required: true },
				];
			default:
				return [
					{ key: 'id', label: 'ID', type: 'text' as const, required: true },
					{ key: 'description', label: 'Description', type: 'textarea' as const, required: true },
				];
		}
	});
</script>

<div class="rule-editor">
	<header class="editor-header">
		<h2>✏️ Editing: {schema.id}</h2>
		<span class="editor-kind">{schema.kind}</span>
	</header>

	<form class="editor-form" onsubmit={(e) => { e.preventDefault(); handleSave(); }}>
		{#each editableFields() as field}
			<div class="field-group">
				<label for={field.key}>
					{field.label}
					{#if field.required}<span class="required">*</span>{/if}
				</label>

				{#if field.type === 'textarea'}
					<textarea
						id={field.key}
						value={String(draft[field.key] ?? '')}
						oninput={(e) => updateField(field.key, (e.target as HTMLTextAreaElement).value)}
						rows="3"
					></textarea>
				{:else if field.type === 'checkbox'}
					<label class="checkbox-label">
						<input
							type="checkbox"
							checked={Boolean(draft[field.key])}
							onchange={(e) => updateField(field.key, (e.target as HTMLInputElement).checked)}
						/>
						<span>Enabled</span>
					</label>
				{:else}
					<input
						id={field.key}
						type="text"
						value={String(draft[field.key] ?? '')}
						oninput={(e) => updateField(field.key, (e.target as HTMLInputElement).value)}
					/>
				{/if}
			</div>
		{/each}

		<!-- Contract section for rules -->
		{#if schema.kind === 'rule'}
			<div class="contract-section">
				<h3>📋 Contract</h3>
				<div class="contract-stats">
					<span class="stat">
						{draft.contractExamples ?? 0} examples
					</span>
					<span class="stat">
						{draft.contractInvariants ?? 0} invariants
					</span>
				</div>
				<p class="contract-note">
					Contract editing requires the full Contract Editor (Phase 2b).
					Current contract examples and invariants are preserved on save.
				</p>
			</div>
		{/if}

		<!-- Live JSON preview -->
		<details class="json-preview">
			<summary>JSON Preview</summary>
			<pre>{JSON.stringify(draft, null, 2)}</pre>
		</details>

		<!-- Validation errors -->
		{#if validation.errors.length > 0}
			<div class="validation-errors">
				<h4>⚠️ Validation Errors</h4>
				<ul>
					{#each validation.errors as error}
						<li>{error}</li>
					{/each}
				</ul>
			</div>
		{/if}

		<!-- Actions -->
		<div class="editor-actions">
			<button type="submit" class="btn-save" disabled={!dirty || !validation.valid}>
				💾 Save & Apply
			</button>
			<button type="button" class="btn-cancel" onclick={onCancel}>
				Cancel
			</button>
			{#if dirty}
				<span class="dirty-indicator">● Unsaved changes</span>
			{/if}
		</div>
	</form>
</div>

<style>
	.rule-editor {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 1.5rem;
	}

	.editor-header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1.5rem;
	}

	.editor-header h2 {
		margin: 0;
		font-size: 1.1rem;
	}

	.editor-kind {
		padding: 0.15rem 0.5rem;
		border-radius: 4px;
		background: var(--color-accent-bg);
		color: var(--color-accent);
		font-size: 0.7rem;
		text-transform: uppercase;
		font-weight: 600;
	}

	.editor-form {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.field-group {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.field-group label {
		font-size: 0.8rem;
		font-weight: 500;
		color: var(--color-text-muted);
	}

	.required { color: var(--color-danger); }

	.field-group input[type="text"],
	.field-group textarea {
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-bg);
		color: var(--color-text);
		font-family: 'JetBrains Mono', 'Fira Code', monospace;
		font-size: 0.85rem;
		resize: vertical;
	}

	.field-group input:focus,
	.field-group textarea:focus {
		outline: none;
		border-color: var(--color-accent);
		box-shadow: 0 0 0 2px var(--color-accent-bg);
	}

	.checkbox-label {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		cursor: pointer;
	}

	.contract-section {
		padding: 1rem;
		border: 1px dashed var(--color-border);
		border-radius: 6px;
		margin-top: 0.5rem;
	}

	.contract-section h3 {
		margin: 0 0 0.5rem;
		font-size: 0.9rem;
	}

	.contract-stats {
		display: flex;
		gap: 1rem;
		margin-bottom: 0.5rem;
	}

	.stat {
		font-size: 0.8rem;
		padding: 0.15rem 0.5rem;
		background: var(--color-hover);
		border-radius: 4px;
	}

	.contract-note {
		font-size: 0.75rem;
		color: var(--color-text-muted);
		margin: 0;
	}

	.json-preview {
		margin-top: 0.5rem;
	}

	.json-preview summary {
		cursor: pointer;
		font-size: 0.8rem;
		color: var(--color-text-muted);
	}

	.json-preview pre {
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 0.75rem;
		font-size: 0.75rem;
		overflow-x: auto;
		margin-top: 0.5rem;
	}

	.validation-errors {
		background: rgba(220, 38, 38, 0.1);
		border: 1px solid var(--color-danger);
		border-radius: 6px;
		padding: 0.75rem;
	}

	.validation-errors h4 {
		margin: 0 0 0.5rem;
		font-size: 0.85rem;
		color: var(--color-danger);
	}

	.validation-errors ul {
		margin: 0;
		padding-left: 1.25rem;
		font-size: 0.8rem;
		color: var(--color-danger);
	}

	.editor-actions {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-top: 0.5rem;
	}

	.btn-save {
		padding: 0.5rem 1.25rem;
		border: none;
		border-radius: 6px;
		background: var(--color-accent);
		color: white;
		cursor: pointer;
		font-weight: 500;
	}

	.btn-save:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.btn-cancel {
		padding: 0.5rem 1rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: transparent;
		color: var(--color-text);
		cursor: pointer;
	}

	.dirty-indicator {
		font-size: 0.75rem;
		color: var(--color-accent);
		margin-left: auto;
	}
</style>
