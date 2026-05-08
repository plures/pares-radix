<script lang="ts">
	import type { Snippet } from 'svelte';
	interface LinkProps { href: string; external?: boolean; class?: string; }

	let {
		href,
		external = false,
		class: className = '',
		children
	}: LinkProps & { children?: Snippet } = $props();
</script>

<a
	class="link {className}"
	{href}
	target={external ? '_blank' : undefined}
	rel={external ? 'noopener noreferrer' : undefined}
>
	{#if children}{@render children()}{/if}
</a>

<style>
	.link {
		color: var(--color-accent, #6366f1);
		text-decoration: none;
		cursor: pointer;
		transition: opacity 0.12s;
	}
	.link:hover {
		text-decoration: underline;
		opacity: 0.85;
	}
</style>
