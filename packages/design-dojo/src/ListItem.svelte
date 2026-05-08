<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { ListItemProps } from './types.js';

	let {
		class: className = '',
		children
	}: ListItemProps & { children?: Snippet } = $props();
</script>

<li class="list-item {className}">
	{#if children}{@render children()}{/if}
</li>

<style>
	.list-item {
		padding: 2px 0;
	}
</style>
