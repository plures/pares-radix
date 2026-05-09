<script lang="ts">
	import { untrack } from 'svelte';
	import { query, emitFact } from '$lib/stores/praxis-svelte.svelte.js';
	import type { DesignSchema } from '$lib/praxis/design.js';
	import { Box, Heading, Text, Button, Input, TextArea, List, ListItem, CodeBlock } from '@plures/design-dojo';

	interface Props {
		schema: DesignSchema;
		onSave: (definition: Record<string, unknown>) => void;
		onCancel: () => void;
	}

	let { schema, onSave, onCancel }: Props = $props();

	// Draft state
	// eslint-disable-next-line plures/no-raw-stores
	let draft = $state<Record<string, unknown>>(untrack(() => ({ ...schema.definition })));
	// eslint-disable-next-line plures/no-raw-stores
	let dirty = $state(false);

	// Validation from praxis
	// eslint-disable-next-line plures/no-raw-stores
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
	// eslint-disable-next-line plures/no-raw-stores
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

<Box class="rule-editor">
	<Box as="header" class="editor-header">
		<Heading level={2}>✏️ Editing: {schema.id}</Heading>
		<Text as="span" class="editor-kind">{schema.kind}</Text>
	</Box>

	<Box as="form" class="editor-form" onclick={(e: MouseEvent) => { e.preventDefault(); handleSave(); }}>
		{#each editableFields() as field}
			<Box class="field-group">
				{#if field.type === 'textarea'}
					<TextArea
						label={field.label}
						value={String(draft[field.key] ?? '')}
						oninput={(e: Event) => updateField(field.key, (e.target as HTMLTextAreaElement).value)}
						rows={3}
					/>
				{:else if field.type === 'checkbox'}
					<Input
						label={field.label}
						type="checkbox"
						checked={Boolean(draft[field.key])}
						onchange={(e: Event) => updateField(field.key, (e.target as HTMLInputElement).checked)}
					/>
				{:else}
					<Input
						label={field.label}
						type="text"
						value={String(draft[field.key] ?? '')}
						oninput={(e: Event) => updateField(field.key, (e.target as HTMLInputElement).value)}
					/>
				{/if}
			</Box>
		{/each}

		<!-- Contract section for rules -->
		{#if schema.kind === 'rule'}
			<Box class="contract-section">
				<Heading level={3}>📋 Contract</Heading>
				<Box class="contract-stats">
					<Text as="span" class="stat">
						{draft.contractExamples ?? 0} examples
					</Text>
					<Text as="span" class="stat">
						{draft.contractInvariants ?? 0} invariants
					</Text>
				</Box>
				<Text as="p" class="contract-note">
					Contract editing requires the full Contract Editor (Phase 2b).
					Current contract examples and invariants are preserved on save.
				</Text>
			</Box>
		{/if}

		<!-- Live JSON preview -->
		<Box as="details" class="json-preview">
			<Box as="summary" class="json-summary">JSON Preview</Box>
			<CodeBlock>{JSON.stringify(draft, null, 2)}</CodeBlock>
		</Box>

		<!-- Validation errors -->
		{#if validation.errors.length > 0}
			<Box class="validation-errors">
				<Heading level={4}>⚠️ Validation Errors</Heading>
				<List>
					{#each validation.errors as error}
						<ListItem>{error}</ListItem>
					{/each}
				</List>
			</Box>
		{/if}

		<!-- Actions -->
		<Box class="editor-actions">
			<Button type="submit" class="btn-save" disabled={!dirty || !validation.valid}>
				💾 Save & Apply
			</Button>
			<Button type="button" class="btn-cancel" onclick={onCancel}>
				Cancel
			</Button>
			{#if dirty}
				<Text as="span" class="dirty-indicator">● Unsaved changes</Text>
			{/if}
		</Box>
	</Box>
</Box>

<style>
	:global(.rule-editor) {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 1.5rem;
	}

	:global(.editor-header) {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1.5rem;
	}

	:global(.editor-header h2) {
		margin: 0;
		font-size: 1.1rem;
	}

	:global(.editor-kind) {
		padding: 0.15rem 0.5rem;
		border-radius: 4px;
		background: var(--color-accent-bg);
		color: var(--color-accent);
		font-size: 0.7rem;
		text-transform: uppercase;
		font-weight: 600;
	}

	:global(.editor-form) {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	:global(.field-group) {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	:global(.required) { color: var(--color-danger); }

	:global(.field-group input[type="text"]),
	:global(.field-group textarea) {
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-bg);
		color: var(--color-text);
		font-family: 'JetBrains Mono', 'Fira Code', monospace;
		font-size: 0.85rem;
		resize: vertical;
	}

	:global(.field-group input:focus),
	:global(.field-group textarea:focus) {
		outline: none;
		border-color: var(--color-accent);
		box-shadow: 0 0 0 2px var(--color-accent-bg);
	}

	:global(.contract-section) {
		padding: 1rem;
		border: 1px dashed var(--color-border);
		border-radius: 6px;
		margin-top: 0.5rem;
	}

	:global(.contract-section h3) {
		margin: 0 0 0.5rem;
		font-size: 0.9rem;
	}

	:global(.contract-stats) {
		display: flex;
		gap: 1rem;
		margin-bottom: 0.5rem;
	}

	:global(.stat) {
		font-size: 0.8rem;
		padding: 0.15rem 0.5rem;
		background: var(--color-hover);
		border-radius: 4px;
	}

	:global(.contract-note) {
		font-size: 0.75rem;
		color: var(--color-text-muted);
		margin: 0;
	}

	:global(.json-preview) {
		margin-top: 0.5rem;
	}

	:global(.json-summary) {
		cursor: pointer;
		font-size: 0.8rem;
		color: var(--color-text-muted);
	}

	:global(.json-preview pre) {
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 0.75rem;
		font-size: 0.75rem;
		overflow-x: auto;
		margin-top: 0.5rem;
	}

	:global(.validation-errors) {
		background: rgba(220, 38, 38, 0.1);
		border: 1px solid var(--color-danger);
		border-radius: 6px;
		padding: 0.75rem;
	}

	:global(.validation-errors h4) {
		margin: 0 0 0.5rem;
		font-size: 0.85rem;
		color: var(--color-danger);
	}

	:global(.validation-errors ul) {
		margin: 0;
		padding-left: 1.25rem;
		font-size: 0.8rem;
		color: var(--color-danger);
	}

	:global(.editor-actions) {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-top: 0.5rem;
	}

	:global(.btn-save) {
		padding: 0.5rem 1.25rem;
		border: none;
		border-radius: 6px;
		background: var(--color-accent);
		color: white;
		cursor: pointer;
		font-weight: 500;
	}

	:global(.btn-save:disabled) {
		opacity: 0.5;
		cursor: not-allowed;
	}

	:global(.btn-cancel) {
		padding: 0.5rem 1rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: transparent;
		color: var(--color-text);
		cursor: pointer;
	}

	:global(.dirty-indicator) {
		font-size: 0.75rem;
		color: var(--color-accent);
		margin-left: auto;
	}
</style>
