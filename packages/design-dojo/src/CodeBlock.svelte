<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { CodeBlockProps } from './types.js';

	let {
		code = '',
		language = '',
		class: className = '',
		children
	}: CodeBlockProps & { children?: Snippet } = $props();
</script>

<pre class="codeblock {className}" data-language={language}><code>{#if children}{@render children()}{:else}{code}{/if}</code></pre>

<style>
	.codeblock {
		padding: 12px 16px;
		border-radius: 8px;
		background: var(--color-surface-alt, #1e1e2e);
		color: var(--color-text, #cdd6f4);
		font-family: 'JetBrains Mono', 'Fira Code', monospace;
		font-size: 0.82rem;
		line-height: 1.6;
		overflow-x: auto;
		margin: 0;
		border: 1px solid var(--color-border);
	}
	code {
		background: none;
		padding: 0;
	}
</style>
