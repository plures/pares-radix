<script lang="ts">
	import type { FieldInfo } from '$lib/plugins/plugin-api.js';

	interface Props {
		fields: FieldInfo[];
		entity: Record<string, unknown>;
		onEdit: () => void;
		onDelete: () => void;
		onBack: () => void;
	}

	let { fields, entity, onEdit, onDelete, onBack }: Props = $props();

	let visibleFields = $derived(fields.filter((f) => !f.name.startsWith('_')));
</script>

<div class="entity-detail">
	<button class="back-btn" onclick={onBack}>← Back to list</button>

	<div class="detail-card">
		<dl>
			{#each visibleFields as field}
				<div class="field-row">
					<dt>{field.name}</dt>
					<dd>{entity[field.name] ?? '—'}</dd>
				</div>
			{/each}
		</dl>

		<div class="detail-meta">
			{#if entity._created_at}
				<span class="meta">Created: {entity._created_at}</span>
			{/if}
			{#if entity._updated_at}
				<span class="meta">Updated: {entity._updated_at}</span>
			{/if}
		</div>

		<div class="detail-actions">
			<button class="edit-btn" onclick={onEdit}>✏️ Edit</button>
			<button class="delete-btn" onclick={onDelete}>🗑️ Delete</button>
		</div>
	</div>
</div>

<style>
	.entity-detail { margin-top: 1rem; }
	.back-btn { all: unset; cursor: pointer; color: var(--color-text-muted); font-size: 0.9rem; margin-bottom: 1rem; display: block; }
	.back-btn:hover { color: var(--color-accent); }
	.detail-card { padding: 1.25rem; background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; }
	dl { margin: 0; }
	.field-row { display: flex; padding: 0.5rem 0; border-bottom: 1px solid var(--color-border); }
	.field-row:last-child { border-bottom: none; }
	dt { width: 140px; font-weight: 500; font-size: 0.85rem; color: var(--color-text-muted); flex-shrink: 0; }
	dd { margin: 0; font-size: 0.9rem; }
	.detail-meta { margin-top: 1rem; display: flex; gap: 1rem; }
	.meta { font-size: 0.8rem; color: var(--color-text-muted); }
	.detail-actions { display: flex; gap: 0.75rem; margin-top: 1rem; }
	.edit-btn, .delete-btn {
		all: unset; cursor: pointer; padding: 0.5rem 1rem; border-radius: 6px; font-size: 0.9rem;
	}
	.edit-btn { border: 1px solid var(--color-border); }
	.edit-btn:hover { border-color: var(--color-accent); }
	.delete-btn { color: var(--color-danger); }
	.delete-btn:hover { background: rgba(220, 38, 38, 0.1); }
</style>
