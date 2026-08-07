<!--
  SchemaDiffView — visual renderer for `schema-delta.ts`'s typed `SchemaDelta`
  operation vocabulary (add_field / update_field / rename_field / retype_field /
  remove_field / reorder_field). Used to review a batch of pending schema
  customizations (ADR-0031 §6) — from `SchemaDesigner`'s `ondelta` stream or a
  replayed migration log — before the host persists them + attaches a `.px`
  migration rule. Pure display: never calls `applyDelta` itself.
-->
<script lang="ts">
	import type { SchemaDiffViewProps, SchemaDelta, SchemaField } from './types-local.js';

	let { deltas, baseSchema, class: className = '' }: SchemaDiffViewProps = $props();

	function fieldByName(name: string): SchemaField | undefined {
		return baseSchema?.fields.find((f) => f.name === name);
	}

	function opLabel(op: SchemaDelta['op']): string {
		switch (op) {
			case 'add_field':
				return 'Add field';
			case 'update_field':
				return 'Update field';
			case 'rename_field':
				return 'Rename field';
			case 'retype_field':
				return 'Change type';
			case 'remove_field':
				return 'Remove field';
			case 'reorder_field':
				return 'Reorder field';
		}
	}

	function opGlyph(op: SchemaDelta['op']): string {
		switch (op) {
			case 'add_field':
				return '+';
			case 'update_field':
				return '~';
			case 'rename_field':
				return '→';
			case 'retype_field':
				return '⇄';
			case 'remove_field':
				return '−';
			case 'reorder_field':
				return '↕';
		}
	}

	function subjectName(delta: SchemaDelta): string {
		switch (delta.op) {
			case 'add_field':
				return delta.field.name;
			case 'update_field':
				return delta.name;
			case 'rename_field':
				return delta.from;
			case 'retype_field':
				return delta.name;
			case 'remove_field':
				return delta.name;
			case 'reorder_field':
				return delta.name;
		}
	}

	function summary(delta: SchemaDelta): string {
		switch (delta.op) {
			case 'add_field':
				return `type: ${delta.field.type}${delta.field.required ? ', required' : ''}`;
			case 'update_field': {
				const prior = fieldByName(delta.name);
				if (prior && prior.type !== delta.field.type) {
					return `${prior.type} → ${delta.field.type}`;
				}
				return `type: ${delta.field.type}`;
			}
			case 'rename_field':
				return `${delta.from} → ${delta.to}`;
			case 'retype_field': {
				const prior = fieldByName(delta.name);
				return prior ? `${prior.type} → ${delta.to}` : `→ ${delta.to}`;
			}
			case 'remove_field':
				return 'field removed';
			case 'reorder_field':
				return `moved to position ${delta.toIndex + 1}`;
		}
	}
</script>

<ol class="schema-diff-view {className}" aria-label="Schema changes">
	{#if deltas === undefined}
		<li class="skeleton" aria-hidden="true">
			<div class="skeleton-row"></div>
			<div class="skeleton-row"></div>
			<div class="skeleton-row"></div>
		</li>
	{:else if deltas.length === 0}
		<li class="empty">
			<p>No schema changes pending.</p>
		</li>
	{:else}
		{#each deltas as delta (delta.op + ':' + subjectName(delta) + ':' + summary(delta))}
			<li class="delta-row op-{delta.op}">
				<span class="delta-glyph" aria-hidden="true">{opGlyph(delta.op)}</span>
				<div class="delta-body">
					<span class="delta-op">{opLabel(delta.op)}</span>
					<span class="delta-subject">{subjectName(delta)}</span>
					<span class="delta-summary">{summary(delta)}</span>
				</div>
			</li>
		{/each}
	{/if}
</ol>

<style>
	.schema-diff-view {
		display: flex;
		flex-direction: column;
		list-style: none;
		margin: 0;
		padding: 0;
		gap: 4px;
	}

	.delta-row {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 8px 10px;
		border-radius: 6px;
		border: 1px solid var(--color-border, #2d3140);
		background: var(--color-surface, #1a1d27);
	}

	.delta-glyph {
		flex: 0 0 auto;
		width: 20px;
		height: 20px;
		display: flex;
		align-items: center;
		justify-content: center;
		font-weight: 700;
		font-size: 0.85rem;
		border-radius: 4px;
		color: var(--color-text-muted, #8b92a5);
		background: color-mix(in srgb, var(--color-text-muted, #8b92a5) 12%, transparent);
	}

	.op-add_field .delta-glyph {
		color: var(--color-success, #22c55e);
		background: color-mix(in srgb, var(--color-success, #22c55e) 15%, transparent);
	}

	.op-remove_field .delta-glyph {
		color: var(--color-danger, #ef4444);
		background: color-mix(in srgb, var(--color-danger, #ef4444) 15%, transparent);
	}

	.op-rename_field .delta-glyph,
	.op-retype_field .delta-glyph {
		color: var(--color-accent, #6366f1);
		background: color-mix(in srgb, var(--color-accent, #6366f1) 15%, transparent);
	}

	.delta-body {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.delta-op {
		font-size: 0.7rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		color: var(--color-text-muted, #8b92a5);
	}

	.delta-subject {
		font-size: 0.85rem;
		font-weight: 600;
		font-family: var(--font-mono, monospace);
		color: var(--color-text, #e2e5eb);
	}

	.delta-summary {
		font-size: 0.75rem;
		color: var(--color-text-muted, #8b92a5);
	}

	.op-remove_field .delta-subject {
		text-decoration: line-through;
	}

	.empty {
		padding: 16px;
		color: var(--color-text-muted, #8b92a5);
		font-size: 0.85rem;
	}

	.skeleton {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.skeleton-row {
		height: 40px;
		border-radius: 6px;
		background: linear-gradient(
			90deg,
			var(--color-surface, #1a1d27) 25%,
			color-mix(in srgb, var(--color-surface, #1a1d27) 60%, var(--color-border, #2d3140)) 50%,
			var(--color-surface, #1a1d27) 75%
		);
		background-size: 200% 100%;
	}

	@media (prefers-reduced-motion: no-preference) {
		.skeleton-row {
			animation: shimmer 1.4s ease-in-out infinite;
		}
	}

	@keyframes shimmer {
		0% { background-position: 200% 0; }
		100% { background-position: -200% 0; }
	}
</style>
