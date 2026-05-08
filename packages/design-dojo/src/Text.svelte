<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { TextProps } from './types.js';

	let {
		as = 'span',
		size,
		weight,
		color,
		truncate = false,
		class: className = '',
		children
	}: TextProps & { children?: Snippet } = $props();
</script>

<svelte:element this={as}
	class="text {className}"
	class:truncate
	style:font-size={size}
	style:font-weight={weight}
	style:color
>
	{#if children}{@render children()}{/if}
</svelte:element>

<style>
	.text {
		margin: 0;
		line-height: 1.5;
	}
	.truncate {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
