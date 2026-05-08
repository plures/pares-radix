<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { BoxProps } from './types.js';

	let {
		as = 'div',
		padding,
		gap,
		direction = 'column',
		align,
		justify,
		wrap = false,
		class: className = '',
		children,
		onclick
	}: BoxProps & { children?: Snippet } = $props();
</script>

<svelte:element this={as}
	class="box {direction} {className}"
	class:wrap
	style:padding
	style:gap
	style:align-items={align}
	style:justify-content={justify}
	{onclick}
	role={onclick ? 'button' : undefined}
	tabindex={onclick ? 0 : undefined}
>
	{#if children}{@render children()}{/if}
</svelte:element>

<style>
	.box {
		display: flex;
		box-sizing: border-box;
	}
	.column { flex-direction: column; }
	.row { flex-direction: row; }
	.wrap { flex-wrap: wrap; }
</style>
