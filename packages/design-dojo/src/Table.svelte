<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { TableProps } from './types.js';

	let {
		class: className = '',
		children
	}: TableProps & { children?: Snippet } = $props();
</script>

<div class="table-wrapper">
	<table class="table {className}">
		{#if children}{@render children()}{/if}
	</table>
</div>

<style>
	.table-wrapper {
		overflow-x: auto;
		border-radius: 8px;
		border: 1px solid var(--color-border);
	}
	.table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.85rem;
		color: var(--color-text);
	}
	.table :global(th) {
		text-align: left;
		padding: 8px 12px;
		font-weight: 600;
		background: var(--color-surface-alt, rgba(0,0,0,0.15));
		border-bottom: 1px solid var(--color-border);
	}
	.table :global(td) {
		padding: 8px 12px;
		border-bottom: 1px solid var(--color-border);
	}
	.table :global(tr:last-child td) {
		border-bottom: none;
	}
	.table :global(tr:hover td) {
		background: var(--color-hover, rgba(0,0,0,0.05));
	}
</style>
