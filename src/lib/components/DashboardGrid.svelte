<script lang="ts">
	import type { DashboardWidget } from '$lib/types/plugin.js';

	interface Props {
		widgets: DashboardWidget[];
	}

	let { widgets }: Props = $props();
</script>

<div class="dashboard-grid">
	{#each widgets as widget}
		<div class="widget" style="grid-column: span {widget.colspan ?? 1}">
			<h3 class="widget-title">{widget.title}</h3>
			<div class="widget-content">
				{#await widget.component() then mod}
					<mod.default />
				{:catch}
					<p class="widget-error">Failed to load widget</p>
				{/await}
			</div>
		</div>
	{/each}

	{#if widgets.length === 0}
		<div class="empty">
			<p>No dashboard widgets registered yet.</p>
			<p class="muted">Plugins add widgets to this dashboard.</p>
		</div>
	{/if}
</div>

<style>
	.dashboard-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
		gap: 16px;
	}

	.widget {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 16px;
	}

	.widget-title {
		margin: 0 0 12px;
		font-size: 0.95rem;
		color: var(--color-text);
	}

	.widget-error {
		color: var(--color-danger);
		font-size: 0.85rem;
	}

	.empty {
		grid-column: 1 / -1;
		text-align: center;
		padding: 48px;
		color: var(--color-text-muted);
	}

	.muted {
		font-size: 0.85rem;
		opacity: 0.7;
	}
</style>
