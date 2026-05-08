<script lang="ts">
	import type { Snippet } from 'svelte';
	interface HeadingProps { level?: 1 | 2 | 3 | 4 | 5 | 6; class?: string; }

	let {
		level = 2,
		class: className = '',
		children
	}: HeadingProps & { children?: Snippet } = $props();

	const tag = `h${level}` as 'h1' | 'h2' | 'h3' | 'h4' | 'h5' | 'h6';
</script>

<svelte:element this={tag} class="heading h{level} {className}">
	{#if children}{@render children()}{/if}
</svelte:element>

<style>
	.heading {
		margin: 0;
		font-weight: 600;
		color: var(--color-text);
		line-height: 1.3;
	}
	.h1 { font-size: 1.75rem; }
	.h2 { font-size: 1.4rem; }
	.h3 { font-size: 1.15rem; }
	.h4 { font-size: 1rem; }
	.h5 { font-size: 0.9rem; }
	.h6 { font-size: 0.8rem; }
</style>
