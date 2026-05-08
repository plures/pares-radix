<script lang="ts">
	import type { SelectProps } from './types.js';

	let {
		value = $bindable(''),
		options,
		disabled = false,
		required = false,
		name,
		label,
		placeholder,
		class: className = '',
		onchange
	}: SelectProps = $props();
</script>

<div class="select-wrapper {className}">
	{#if label}
		<label class="select-label" for={name}>{label}</label>
	{/if}
	<select
		class="select"
		bind:value
		{disabled}
		{required}
		{name}
		id={name}
		{onchange}
	>
		{#if placeholder}
			<option value="" disabled selected>{placeholder}</option>
		{/if}
		{#each options as opt}
			<option value={opt.value}>{opt.label}</option>
		{/each}
	</select>
</div>

<style>
	.select-wrapper {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.select-label {
		font-size: 0.8rem;
		font-weight: 500;
		color: var(--color-text-muted, #888);
	}
	.select {
		padding: 7px 10px;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-text);
		font-size: 0.85rem;
		outline: none;
		cursor: pointer;
		transition: border-color 0.12s;
	}
	.select:focus {
		border-color: var(--color-accent, #6366f1);
	}
	.select:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}
</style>
