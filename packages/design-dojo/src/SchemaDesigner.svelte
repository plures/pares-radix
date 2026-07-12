<script lang="ts">
	import FieldEditor from './FieldEditor.svelte';
	import { applyDelta } from './schema-delta.js';
	import type {
		EntitySchema,
		SchemaDesignerProps,
		SchemaField,
		SchemaDelta
	} from './types-local.js';

	let {
		schema,
		disabled = false,
		onschemachange,
		ondelta,
		class: className = ''
	}: SchemaDesignerProps = $props();

	// `editing` is the name of the field open in the inline editor, or the
	// sentinel '' + `creating=true` for a brand-new field. Controlled component:
	// we never mutate `schema`; every edit emits a delta the host persists.
	let creating = $state(false);
	let editingName = $state<string | null>(null);

	const fieldNames = $derived(schema.fields.map((f) => f.name));

	function fieldLabel(field: SchemaField): string {
		if (field.label) return field.label;
		return field.name
			.replace(/[_-]+/g, ' ')
			.replace(/([a-z])([A-Z])/g, '$1 $2')
			.replace(/^\w/, (c) => c.toUpperCase());
	}

	const editingField = $derived.by<SchemaField | undefined>(() =>
		editingName ? schema.fields.find((f) => f.name === editingName) : undefined
	);

	/** Single choke-point: apply, notify delta + new schema, close editors. */
	function emit(delta: SchemaDelta) {
		let next: EntitySchema;
		try {
			next = applyDelta(schema, delta);
		} catch (err) {
			// Reject invalid ops (dup/unknown) without touching host state.
			// Surface via console; host validation is authoritative.
			console.warn('[SchemaDesigner] rejected delta', delta, err);
			return;
		}
		ondelta?.(delta);
		onschemachange?.(next);
	}

	function startAdd() {
		creating = true;
		editingName = null;
	}
	function startEdit(name: string) {
		editingName = name;
		creating = false;
	}
	function cancelEdit() {
		creating = false;
		editingName = null;
	}

	function handleFieldSubmit(after: SchemaField) {
		if (creating) {
			emit({ op: 'add_field', field: after });
		} else if (editingName) {
			emit({ op: 'update_field', name: editingName, field: after });
		}
		cancelEdit();
	}

	function removeField(name: string) {
		emit({ op: 'remove_field', name });
		if (editingName === name) cancelEdit();
	}

	function move(name: string, dir: -1 | 1) {
		const i = fieldNames.indexOf(name);
		const toIndex = i + dir;
		if (toIndex < 0 || toIndex >= schema.fields.length) return;
		emit({ op: 'reorder_field', name, toIndex });
	}
</script>

<div class="schema-designer {className}">
	<header class="sd-header">
		<h3 class="sd-title">{schema.name ?? 'Schema'}</h3>
		<button
			type="button"
			class="btn btn-primary"
			{disabled}
			onclick={startAdd}
		>
			+ Add field
		</button>
	</header>

	{#if schema.fields.length === 0}
		<p class="sd-empty">No fields yet. Add one to define this entity.</p>
	{/if}

	<ul class="sd-list">
		{#each schema.fields as field, i (field.name)}
			<li class="sd-row">
				<div class="sd-row-main">
					<div class="sd-row-info">
						<span class="sd-name">{fieldLabel(field)}</span>
						<span class="sd-meta">
							<code>{field.name}</code>
							· {field.type}{field.required ? ' · required' : ''}
						</span>
					</div>
					<div class="sd-row-actions">
						<button
							type="button"
							class="icon-btn"
							title="Move up"
							disabled={disabled || i === 0}
							onclick={() => move(field.name, -1)}
						>↑</button>
						<button
							type="button"
							class="icon-btn"
							title="Move down"
							disabled={disabled || i === schema.fields.length - 1}
							onclick={() => move(field.name, 1)}
						>↓</button>
						<button
							type="button"
							class="icon-btn"
							title="Edit"
							{disabled}
							onclick={() => startEdit(field.name)}
						>✎</button>
						<button
							type="button"
							class="icon-btn danger"
							title="Remove"
							{disabled}
							onclick={() => removeField(field.name)}
						>✕</button>
					</div>
				</div>

				{#if editingName === field.name}
					<div class="sd-editor">
						<FieldEditor
							field={editingField}
							existingNames={fieldNames}
							submitLabel="Update field"
							{disabled}
							onsubmit={handleFieldSubmit}
							oncancel={cancelEdit}
						/>
					</div>
				{/if}
			</li>
		{/each}
	</ul>

	{#if creating}
		<div class="sd-editor sd-editor-new">
			<h4 class="sd-editor-title">New field</h4>
			<FieldEditor
				existingNames={fieldNames}
				submitLabel="Add field"
				{disabled}
				onsubmit={handleFieldSubmit}
				oncancel={cancelEdit}
			/>
		</div>
	{/if}
</div>

<style>
	.schema-designer {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.sd-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}
	.sd-title {
		margin: 0;
		font-size: 1rem;
		color: var(--color-text);
	}
	.sd-empty {
		margin: 0;
		font-size: 0.85rem;
		color: var(--color-text-muted);
	}
	.sd-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.sd-row {
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: var(--color-surface);
	}
	.sd-row-main {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		padding: 8px 10px;
	}
	.sd-row-info {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.sd-name {
		font-size: 0.9rem;
		font-weight: 500;
		color: var(--color-text);
	}
	.sd-meta {
		font-size: 0.75rem;
		color: var(--color-text-muted);
	}
	.sd-meta code {
		font-size: 0.72rem;
	}
	.sd-row-actions {
		display: flex;
		gap: 4px;
		flex-shrink: 0;
	}
	.icon-btn {
		width: 26px;
		height: 26px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: transparent;
		color: var(--color-text);
		font-size: 0.85rem;
		cursor: pointer;
		transition: background 0.12s, border-color 0.12s;
	}
	.icon-btn:hover:not(:disabled) {
		background: var(--color-hover);
	}
	.icon-btn.danger:hover:not(:disabled) {
		border-color: var(--color-danger, #ef4444);
		color: var(--color-danger, #ef4444);
	}
	.icon-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
	.sd-editor {
		padding: 10px;
		border-top: 1px solid var(--color-border);
	}
	.sd-editor-new {
		border: 1px dashed var(--color-border);
		border-radius: 8px;
	}
	.sd-editor-title {
		margin: 0 0 8px;
		font-size: 0.85rem;
		color: var(--color-text-muted);
	}
	.btn {
		border-radius: 6px;
		padding: 7px 14px;
		font-size: 0.85rem;
		font-weight: 500;
		cursor: pointer;
		border: 1px solid transparent;
		transition: background 0.12s;
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
</style>
