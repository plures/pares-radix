<script lang="ts">
	import type { StatusBarProps } from './types.js';

	let { items = [] }: StatusBarProps = $props();

	// Split items into left-side and right-side; last item goes right.
	let leftItems = $derived(items.length > 1 ? items.slice(0, -1) : items);
	let rightItems = $derived(items.length > 1 ? items.slice(-1) : []);
</script>

<footer class="status-bar" aria-label="Status bar">
	<div class="status-left">
		{#each leftItems as item (item.label)}
			<span class="status-item">
				<span class="status-label">{item.label}</span>
				<span class="status-value">{item.value}</span>
			</span>
		{/each}
	</div>

	<div class="status-right">
		{#each rightItems as item (item.label)}
			<span class="status-item">
				<span class="status-label">{item.label}</span>
				<span class="status-value">{item.value}</span>
			</span>
		{/each}
	</div>
</footer>

<style>
	.status-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 4px 12px;
		border-top: 1px solid var(--color-border);
		background: var(--color-surface);
		font-size: 0.75rem;
		color: var(--color-text-muted);
		height: 28px;
	}

	.status-left, .status-right {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.status-item { display: flex; gap: 4px; align-items: center; }
	.status-label { color: var(--color-text-muted); }
	.status-value { color: var(--color-text); font-weight: 500; }
</style>
