<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { ButtonProps } from './types.js';

	let {
		variant = 'secondary',
		disabled = false,
		onclick,
		class: className = '',
		style,
		children
	}: ButtonProps & { children: Snippet } = $props();
</script>

<button
	class="btn {variant} {className}"
	{disabled}
	{onclick}
	{style}
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

	.btn.danger {
		background: var(--color-danger, #ef4444);
		color: #fff;
		border-color: transparent;
	}

	.btn.danger:hover { filter: brightness(1.1); }

	.btn.ghost {
		background: transparent;
		border-color: transparent;
		color: var(--color-text-muted, #888);
	}

	.btn.ghost:hover {
		background: var(--color-hover);
		color: var(--color-text);
	}

	.btn:disabled { opacity: 0.55; cursor: not-allowed; }
</style>
