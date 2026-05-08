<script lang="ts">
	import { Box, Button, Text } from '@plures/design-dojo';
	import type { FieldInfo } from '$lib/plugins/plugin-api.js';

	interface Props {
		fields: FieldInfo[];
		entity: Record<string, unknown>;
		onEdit: () => void;
		onDelete: () => void;
		onBack: () => void;
	}

	let { fields, entity, onEdit, onDelete, onBack }: Props = $props();

	// eslint-disable-next-line plures/no-raw-stores
	let visibleFields = $derived(fields.filter((f) => !f.name.startsWith('_')));
</script>

<Box class="entity-detail">
	<Button class="back-btn" onclick={onBack}>← Back to list</Button>

	<Box class="detail-card">
		<Box as="dl">
			{#each visibleFields as field}
				<Box class="field-row">
					<Box as="dt">{field.name}</Box>
					<Box as="dd">{entity[field.name] ?? '—'}</Box>
				</Box>
			{/each}
		</Box>

		<Box class="detail-meta">
			{#if entity._created_at}
				<Text as="span" class="meta">Created: {entity._created_at}</Text>
			{/if}
			{#if entity._updated_at}
				<Text as="span" class="meta">Updated: {entity._updated_at}</Text>
			{/if}
		</Box>

		<Box class="detail-actions">
			<Button class="edit-btn" onclick={onEdit}>✏️ Edit</Button>
			<Button class="delete-btn" onclick={onDelete}>🗑️ Delete</Button>
		</Box>
	</Box>
</Box>

<style>
	:global(.entity-detail) { margin-top: 1rem; }
	:global(.back-btn) { all: unset; cursor: pointer; color: var(--color-text-muted); font-size: 0.9rem; margin-bottom: 1rem; display: block; }
	:global(.back-btn:hover) { color: var(--color-accent); }
	:global(.detail-card) { padding: 1.25rem; background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; }
	:global(dl) { margin: 0; }
	:global(.field-row) { display: flex; padding: 0.5rem 0; border-bottom: 1px solid var(--color-border); }
	:global(.field-row:last-child) { border-bottom: none; }
	:global(dt) { width: 140px; font-weight: 500; font-size: 0.85rem; color: var(--color-text-muted); flex-shrink: 0; }
	:global(dd) { margin: 0; font-size: 0.9rem; }
	:global(.detail-meta) { margin-top: 1rem; display: flex; gap: 1rem; }
	:global(.meta) { font-size: 0.8rem; color: var(--color-text-muted); }
	:global(.detail-actions) { display: flex; gap: 0.75rem; margin-top: 1rem; }
	:global(.edit-btn), :global(.delete-btn) {
		all: unset; cursor: pointer; padding: 0.5rem 1rem; border-radius: 6px; font-size: 0.9rem;
	}
	:global(.edit-btn) { border: 1px solid var(--color-border); }
	:global(.edit-btn:hover) { border-color: var(--color-accent); }
	:global(.delete-btn) { color: var(--color-danger); }
	:global(.delete-btn:hover) { background: rgba(220, 38, 38, 0.1); }
</style>
