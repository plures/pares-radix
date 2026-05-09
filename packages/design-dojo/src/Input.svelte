<script lang="ts">
	import type { InputProps } from './types-local.js';

	let {
		type = 'text',
		value = $bindable(''),
		placeholder,
		disabled = false,
		required = false,
		name,
		label,
		error,
		class: className = '',
		accept,
		oninput,
		onchange
	}: InputProps = $props();
</script>

<div class="input-wrapper {className}">
	{#if label}
		<label class="input-label" for={name}>{label}</label>
	{/if}
	<input
		class="input"
		class:has-error={!!error}
		{type}
		bind:value
		{placeholder}
		{disabled}
		{required}
		{name}
		id={name}
		{accept}
		{oninput}
		{onchange}
	/>
	{#if error}
		<span class="input-error">{error}</span>
	{/if}
</div>

<style>
	.input-wrapper {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.input-label {
		font-size: 0.8rem;
		font-weight: 500;
		color: var(--color-text-muted, #888);
	}
	.input {
		padding: 7px 10px;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-text);
		font-size: 0.85rem;
		outline: none;
		transition: border-color 0.12s;
	}
	.input:focus {
		border-color: var(--color-accent, #6366f1);
	}
	.input:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}
	.has-error {
		border-color: var(--color-danger, #ef4444);
	}
	.input-error {
		font-size: 0.75rem;
		color: var(--color-danger, #ef4444);
	}
</style>
