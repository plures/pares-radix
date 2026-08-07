<!--
  JSONTreeViewer — collapsible raw-JSON inspector for ad-hoc inspection of
  registry/ADR/audit blobs. Distinct from SchemaForm (validated-form model,
  requires an EntitySchema) and CodeBlock (static pretty-printed text, no
  interaction): this renders an arbitrary, unvalidated JSON value as a
  navigable expand/collapse tree, one node per key/array-index, with a
  depth-limited default-expand so large blobs don't dump everything open.
  Gap-analysis candidate #6 (design-dojo:preemptive-controls-and-improvements).
-->
<script lang="ts">
	import type { JSONValue, JSONTreeViewerProps } from './types-local.js';

	let { data, rootLabel = 'root', defaultExpandDepth = 1, class: className = '' }: JSONTreeViewerProps = $props();

	function isObject(v: JSONValue): v is { [key: string]: JSONValue } {
		return v !== null && typeof v === 'object' && !Array.isArray(v);
	}

	function isArray(v: JSONValue): v is JSONValue[] {
		return Array.isArray(v);
	}

	function typeLabel(v: JSONValue): string {
		if (v === null) return 'null';
		if (isArray(v)) return `array(${v.length})`;
		if (isObject(v)) return `object(${Object.keys(v).length})`;
		return typeof v;
	}

	function entries(v: { [key: string]: JSONValue } | JSONValue[]): [string, JSONValue][] {
		return isArray(v) ? v.map((item, i) => [String(i), item] as [string, JSONValue]) : Object.entries(v);
	}
</script>

{#snippet node(label: string, value: JSONValue, depth: number)}
	{@const expandable = isObject(value) || isArray(value)}
	{#if expandable}
		<details class="tree-node" open={depth < defaultExpandDepth}>
			<summary>
				<span class="node-label">{label}</span>
				<span class="node-type">{typeLabel(value)}</span>
			</summary>
			<div class="node-children">
				{#each entries(value) as [childKey, childValue] (childKey)}
					{@render node(childKey, childValue, depth + 1)}
				{/each}
			</div>
		</details>
	{:else}
		<div class="tree-leaf">
			<span class="node-label">{label}</span>
			<span class="node-value" data-type={value === null ? 'null' : typeof value}>{JSON.stringify(value)}</span>
		</div>
	{/if}
{/snippet}

<div class="json-tree-viewer {className}" aria-label="JSON tree: {rootLabel}">
	{@render node(rootLabel, data, 0)}
</div>

<style>
	.json-tree-viewer {
		font-family: var(--font-mono, monospace);
		font-size: 0.82rem;
		line-height: 1.5;
		color: var(--color-text, #e2e5eb);
	}

	:global(.json-tree-viewer .tree-node) {
		margin-left: 14px;
	}

	:global(.json-tree-viewer .tree-node > summary) {
		cursor: pointer;
		list-style: none;
		display: flex;
		align-items: baseline;
		gap: 6px;
		padding: 1px 0;
	}

	:global(.json-tree-viewer .tree-node > summary::-webkit-details-marker) {
		display: none;
	}

	:global(.json-tree-viewer .tree-node > summary::before) {
		content: '▸';
		color: var(--color-text-muted, #8b92a5);
		font-size: 0.7rem;
		width: 10px;
		display: inline-block;
	}

	:global(.json-tree-viewer .tree-node[open] > summary::before) {
		content: '▾';
	}

	.tree-leaf {
		margin-left: 14px;
		display: flex;
		gap: 6px;
		padding: 1px 0;
	}

	.node-label {
		color: var(--color-accent, #6366f1);
		font-weight: 600;
	}

	.node-type {
		color: var(--color-text-muted, #8b92a5);
		font-size: 0.72rem;
	}

	.node-value {
		color: var(--color-text, #e2e5eb);
	}

	.node-value[data-type='string'] {
		color: var(--color-success, #22c55e);
	}

	.node-value[data-type='number'] {
		color: var(--color-warning, #eab308);
	}

	.node-value[data-type='boolean'] {
		color: var(--color-accent, #6366f1);
	}
</style>
