<!--
  PipelineStageIndicator — discrete named-stage sequence indicator.

  Distinct from `ProgressBar`'s continuous-percent model: expresses "stage 3 of 6
  failed, stages 1-2 passed" for the dev-lifecycle 6-stage gate (design→dev→
  document→QA→deploy→verify) that AGENTS.md itself mandates, and any similar
  discrete staged pipeline (CI stages, deploy gates, praxis lifecycle phases).
-->
<script lang="ts">
	import type { PipelineStageIndicatorProps } from './types-local.js';

	let { stages, orientation = 'horizontal', class: className = '' }: PipelineStageIndicatorProps =
		$props();

	function statusLabel(status: string): string {
		switch (status) {
			case 'passed':
				return 'Passed';
			case 'failed':
				return 'Failed';
			case 'in-progress':
				return 'In progress';
			case 'skipped':
				return 'Skipped';
			default:
				return 'Pending';
		}
	}

	function statusGlyph(status: string): string {
		switch (status) {
			case 'passed':
				return '✓';
			case 'failed':
				return '✕';
			case 'in-progress':
				return '…';
			case 'skipped':
				return '—';
			default:
				return '○';
		}
	}
</script>

<ol
	class="pipeline-stage-indicator {orientation} {className}"
	aria-label="Pipeline stages"
>
	{#if stages.length === 0}
		<li class="empty">
			<p>No pipeline stages defined yet.</p>
		</li>
	{:else}
		{#each stages as stage, i (stage.id)}
			<li
				class="stage status-{stage.status}"
				aria-current={stage.status === 'in-progress' ? 'step' : undefined}
			>
				<div
					class="stage-marker"
					aria-hidden="true"
					title={statusLabel(stage.status)}
				>
					{#if stage.status === 'in-progress'}
						<span class="spinner" aria-hidden="true"></span>
					{:else}
						<span class="glyph">{statusGlyph(stage.status)}</span>
					{/if}
				</div>
				<div class="stage-body">
					<span class="stage-label">{stage.label}</span>
					<span class="stage-status sr-only">{statusLabel(stage.status)}</span>
					{#if stage.detail}
						<span class="stage-detail">{stage.detail}</span>
					{/if}
					{#if stage.status === 'failed' && stage.onRetry}
						<button
							type="button"
							class="stage-retry"
							onclick={() => stage.onRetry?.()}
						>
							Retry
						</button>
					{/if}
				</div>
				{#if i < stages.length - 1}
					<div class="stage-connector" aria-hidden="true"></div>
				{/if}
			</li>
		{/each}
	{/if}
</ol>

<style>
	.pipeline-stage-indicator {
		display: flex;
		list-style: none;
		margin: 0;
		padding: 0;
		gap: 0;
	}

	.pipeline-stage-indicator.horizontal {
		flex-direction: row;
		align-items: flex-start;
		flex-wrap: wrap;
	}

	.pipeline-stage-indicator.vertical {
		flex-direction: column;
	}

	.stage {
		position: relative;
		display: flex;
		align-items: flex-start;
		gap: 8px;
		flex: 1 1 auto;
		min-width: 120px;
	}

	.pipeline-stage-indicator.vertical .stage {
		min-width: 0;
	}

	.stage-marker {
		flex: 0 0 auto;
		width: 28px;
		height: 28px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 0.8rem;
		font-weight: 700;
		border: 2px solid var(--color-border, #2d3140);
		background: var(--color-surface, #1a1d27);
		color: var(--color-text-muted, #8b92a5);
		z-index: 1;
	}

	.status-passed .stage-marker {
		border-color: var(--color-success, #22c55e);
		color: var(--color-success, #22c55e);
	}

	.status-failed .stage-marker {
		border-color: var(--color-danger, #ef4444);
		color: var(--color-danger, #ef4444);
	}

	.status-in-progress .stage-marker {
		border-color: var(--color-accent, #6366f1);
		color: var(--color-accent, #6366f1);
	}

	.status-skipped .stage-marker {
		opacity: 0.5;
	}

	.spinner {
		width: 12px;
		height: 12px;
		border-radius: 50%;
		border: 2px solid var(--color-accent, #6366f1);
		border-top-color: transparent;
	}

	@media (prefers-reduced-motion: no-preference) {
		.spinner {
			animation: spin 0.8s linear infinite;
		}
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	.stage-body {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding-top: 3px;
	}

	.stage-label {
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--color-text, #e2e5eb);
	}

	.status-skipped .stage-label {
		color: var(--color-text-muted, #8b92a5);
		text-decoration: line-through;
	}

	.stage-detail {
		font-size: 0.75rem;
		color: var(--color-text-muted, #8b92a5);
	}

	.status-failed .stage-detail {
		color: var(--color-danger, #ef4444);
	}

	.stage-retry {
		align-self: flex-start;
		margin-top: 4px;
		padding: 3px 10px;
		font-size: 0.75rem;
		font-weight: 500;
		border-radius: 5px;
		border: 1px solid var(--color-danger, #ef4444);
		background: transparent;
		color: var(--color-danger, #ef4444);
		cursor: pointer;
	}

	.stage-retry:hover {
		background: var(--color-danger, #ef4444);
		color: #fff;
	}

	.stage-retry:focus-visible {
		outline: 2px solid var(--color-accent, #6366f1);
		outline-offset: 2px;
	}

	.stage-connector {
		position: absolute;
		background: var(--color-border, #2d3140);
	}

	.pipeline-stage-indicator.horizontal .stage-connector {
		top: 13px;
		left: 28px;
		right: -50%;
		height: 2px;
	}

	.pipeline-stage-indicator.vertical .stage-connector {
		top: 28px;
		left: 13px;
		width: 2px;
		bottom: -8px;
	}

	.status-passed .stage-connector {
		background: var(--color-success, #22c55e);
	}

	.empty {
		padding: 16px;
		color: var(--color-text-muted, #8b92a5);
		font-size: 0.85rem;
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}
</style>
