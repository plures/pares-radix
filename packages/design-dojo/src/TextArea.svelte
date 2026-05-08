<script lang="ts">
	import type { TextAreaProps } from './types.js';

	let {
		value = $bindable(''),
		placeholder,
		disabled = false,
		required = false,
		rows = 4,
		name,
		label,
		error,
		class: className = '',
		oninput,
		onkeydown
	}: TextAreaProps = $props();
</script>

<div class="textarea-wrapper {className}">
	{#if label}
		<label class="textarea-label" for={name}>{label}</label>
	{/if}
	<textarea
		class="textarea"
		class:has-error={!!error}
		bind:value
		{placeholder}
		{disabled}
		{required}
		{rows}
		{name}
		id={name}
		{oninput}
		{onkeydown}
	></textarea>
	{#if error}
		<span class="textarea-error">{error}</span>
	{/if}
</div>

<style>
	.textarea-wrapper {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.textarea-label {
		font-size: 0.8rem;
		font-weight: 500;
		color: var(--color-text-muted, #888);
	}
	.textarea {
		padding: 7px 10px;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-text);
		font-size: 0.85rem;
		font-family: inherit;
		resize: vertical;
		outline: none;
		transition: border-color 0.12s;
	}
	.textarea:focus {
		border-color: var(--color-accent, #6366f1);
	}
	.textarea:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}
	.has-error {
		border-color: var(--color-danger, #ef4444);
	}
	.textarea-error {
		font-size: 0.75rem;
		color: var(--color-danger, #ef4444);
	}
</style>
