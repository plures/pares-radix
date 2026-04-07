<script lang="ts">
	import type { DashboardGridProps } from './types.js';

	let { widgets }: DashboardGridProps = $props();
</script>

<div class="dashboard-grid">
	{#each widgets as widget (widget.id)}
		<div class="widget cs-{Math.min(widget.colspan ?? 1, 4)}">
			<h3 class="widget-title">{widget.title}</h3>
			<div class="widget-content">
				{#await widget.component()}
					<div class="widget-loading" aria-busy="true" aria-label="Loading {widget.title}"></div>
				{:then mod}
					{@const Component = mod.default}
					<Component />
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
	/* Responsive grid: 1 col mobile → 2 col tablet → 4 col desktop */
	.dashboard-grid {
		display: grid;
		grid-template-columns: 1fr;
		gap: 16px;
	}

	@media (min-width: 640px) {
		.dashboard-grid {
			grid-template-columns: repeat(2, 1fr);
		}
	}

	@media (min-width: 1024px) {
		.dashboard-grid {
			grid-template-columns: repeat(4, 1fr);
		}
	}

	/* Colspan classes — each widget spans 1–4 columns */
	.cs-1 { grid-column: span 1; }
	.cs-2 { grid-column: span 2; }
	.cs-3 { grid-column: span 3; }
	.cs-4 { grid-column: span 4; }

	/* Cap colspan to available columns on smaller viewports */
	@media (max-width: 639px) {
		.cs-2, .cs-3, .cs-4 { grid-column: span 1; }
	}

	@media (min-width: 640px) and (max-width: 1023px) {
		.cs-3, .cs-4 { grid-column: span 2; }
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

	/* Shimmer skeleton while the widget component lazy-loads */
	.widget-loading {
		height: 80px;
		border-radius: 4px;
		background: var(--color-border);
	}

	@media (prefers-reduced-motion: no-preference) {
		.widget-loading {
			background: linear-gradient(
				90deg,
				var(--color-border) 25%,
				var(--color-hover) 50%,
				var(--color-border) 75%
			);
			background-size: 200% 100%;
			animation: shimmer 1.5s infinite linear;
		}

		@keyframes shimmer {
			0%   { background-position: 200% 0; }
			100% { background-position: -200% 0; }
		}
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
