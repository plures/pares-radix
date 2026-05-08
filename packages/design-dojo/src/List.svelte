<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { ListProps, ListItemProps } from './types.js';

	let {
		ordered = false,
		class: className = '',
		children
	}: ListProps & { children?: Snippet } = $props();

	const tag = ordered ? 'ol' : 'ul';
</script>

<svelte:element this={tag} class="list {className}">
	{#if children}{@render children()}{/if}
</svelte:element>

<style>
	.list {
		margin: 0;
		padding-left: 1.5em;
		line-height: 1.6;
		color: var(--color-text);
	}
</style>
