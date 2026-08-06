<!--
  PersistentGateBanner — non-dismissible, page-level banner for a hard-gate
  violation that must stay visible until resolved.

  Deliberately distinct from Toast/NotificationStack (transient-by-design):
  this banner has no auto-dismiss timer and no manual close affordance. It
  only disappears when the host removes it after confirming the underlying
  condition has actually cleared (via onResolve), never on its own.
-->
<script lang="ts">
	import type { PersistentGateBannerProps } from './types-local.js';

	let {
		severity,
		label,
		detail,
		blockedItems,
		onResolve,
		resolveLabel = 'Mark resolved',
		class: className = '',
	}: PersistentGateBannerProps = $props();

	const SEVERITY_LABELS: Record<string, string> = {
		blocked: 'Blocked',
		assistance_required: 'Assistance required',
		hard_gate: 'Hard gate',
	};

	let severityLabel = $derived(SEVERITY_LABELS[severity] ?? severity);
	let hasItems = $derived(Boolean(blockedItems && blockedItems.length > 0));
</script>

<div
	class="persistent-gate-banner severity-{severity} {className}"
	role="alert"
	aria-live="assertive"
>
	<div class="gate-icon" aria-hidden="true">⛔</div>

	<div class="gate-body">
		<div class="gate-header">
			<span class="gate-severity-badge severity-{severity}">{severityLabel}</span>
			<span class="gate-label">{label}</span>
		</div>

		{#if detail}
			<p class="gate-detail">{detail}</p>
		{/if}

		{#if hasItems}
			<ul class="gate-items">
				{#each blockedItems ?? [] as item (item)}
					<li class="gate-item">{item}</li>
				{/each}
			</ul>
		{/if}
	</div>

	{#if onResolve}
		<button type="button" class="gate-resolve" onclick={() => onResolve?.()}>
			{resolveLabel}
		</button>
	{/if}
</div>

<style>
	.persistent-gate-banner {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: 14px 16px;
		border-radius: 8px;
		border: 1px solid var(--color-danger, #ef4444);
		background: rgba(239, 68, 68, 0.08);
		color: var(--color-text, #e2e5eb);
	}

	.severity-assistance_required {
		border-color: #eab308;
		background: rgba(234, 179, 8, 0.08);
	}

	.severity-hard_gate {
		border-color: var(--color-danger, #ef4444);
		background: rgba(239, 68, 68, 0.12);
	}

	.gate-icon {
		font-size: 1.1rem;
		line-height: 1.4;
		flex-shrink: 0;
	}

	.gate-body {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-width: 0;
	}

	.gate-header {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.gate-severity-badge {
		font-size: 0.68rem;
		font-weight: 700;
		padding: 2px 8px;
		border-radius: 10px;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		background: rgba(239, 68, 68, 0.18);
		color: var(--color-danger, #ef4444);
		flex-shrink: 0;
	}

	.gate-severity-badge.severity-assistance_required {
		background: rgba(234, 179, 8, 0.18);
		color: #eab308;
	}

	.gate-label {
		font-weight: 600;
		font-size: 0.92rem;
	}

	.gate-detail {
		margin: 0;
		font-size: 0.82rem;
		color: var(--color-text-muted, #8b92a5);
		line-height: 1.4;
	}

	.gate-items {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.gate-item {
		font-size: 0.72rem;
		font-family: var(--font-mono, ui-monospace, monospace);
		padding: 2px 8px;
		border-radius: 5px;
		background: var(--color-hover, rgba(255, 255, 255, 0.06));
		color: var(--color-text-muted, #8b92a5);
	}

	.gate-resolve {
		flex-shrink: 0;
		padding: 6px 14px;
		border-radius: 6px;
		border: 1px solid var(--color-border, #2d3140);
		background: transparent;
		color: var(--color-text, #e2e5eb);
		font-size: 0.8rem;
		cursor: pointer;
		white-space: nowrap;
	}

	.gate-resolve:hover {
		background: var(--color-hover, rgba(255, 255, 255, 0.05));
	}

	.gate-resolve:focus-visible {
		outline: 2px solid var(--color-accent, #6366f1);
		outline-offset: 2px;
	}
</style>
