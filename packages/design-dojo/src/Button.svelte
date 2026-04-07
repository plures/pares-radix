<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { ButtonProps } from './types.js';

	let {
		variant = 'secondary',
		disabled = false,
		onclick,
		children
	}: ButtonProps & { children: Snippet } = $props();
</script>

<button
	class="btn {variant}"
	{disabled}
	{onclick}
>
	{@render children()}
</button>

<style>
	.btn {
		padding: 7px 14px;
		border-radius: 6px;
		font-size: 0.85rem;
		cursor: pointer;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-text);
		font-weight: 500;
		transition: background 0.12s;
	}

	.btn:hover { background: var(--color-hover); }

	.btn.primary {
		background: var(--color-accent, #6366f1);
		color: #fff;
		border-color: transparent;
	}

	.btn.primary:hover { filter: brightness(1.1); }

	.btn.secondary { background: var(--color-surface); }

	.btn:disabled { opacity: 0.55; cursor: not-allowed; }
</style>
