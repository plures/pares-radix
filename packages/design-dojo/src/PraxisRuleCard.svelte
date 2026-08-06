<!--
  PraxisRuleCard — renders one Praxis `.px` constraint/ADR: id, severity badge,
  evidence/condition source, live pass/fail/unknown state, with an
  expand-for-evidence-table affordance. Composite reuse of existing CodeBlock
  (condition source) + Badge (severity + status) + Callout (violation reason)
  primitives — no new interaction metaphor, just a domain-shaped composite.
  Distinct from `PraxisDevOverlay`/`PraxisBox` (design-dojo npm-package repo,
  2026-08-04 batch): this is the lighter-weight, evidence-table-first card
  variant scoped to the pares-radix shim's own consumers (dev-lifecycle gate
  UI, ADO/audit reporting), not a redesign of the overlay component.
-->
<script lang="ts">
	import CodeBlock from './CodeBlock.svelte';
	import { Badge, Callout } from '@plures/design-dojo-npm';
	import type { PraxisRuleCardProps, PraxisEvidenceRow } from './types-local.js';

	let { rule, expanded = $bindable(false), class: className = '' }: PraxisRuleCardProps = $props();

	function severityVariant(severity: string): 'danger' | 'warning' | 'info' {
		switch (severity) {
			case 'error':
				return 'danger';
			case 'warning':
				return 'warning';
			default:
				return 'info';
		}
	}

	function statusVariant(status: string): 'success' | 'danger' | 'neutral' {
		switch (status) {
			case 'pass':
				return 'success';
			case 'fail':
				return 'danger';
			default:
				return 'neutral';
		}
	}

	function calloutTone(status: string): 'error' | 'warning' | 'info' {
		return status === 'fail' ? 'error' : 'warning';
	}

	function statusLabel(status: string): string {
		switch (status) {
			case 'pass':
				return 'Passing';
			case 'fail':
				return 'Failing';
			default:
				return 'Unknown';
		}
	}

	function toggle() {
		expanded = !expanded;
	}
</script>

<div class="praxis-rule-card status-{rule?.status ?? 'unknown'} {className}">
	{#if rule === undefined}
		<div class="skeleton" aria-hidden="true">
			<div class="skeleton-row"></div>
			<div class="skeleton-row"></div>
		</div>
	{:else}
		<div class="card-header">
			<div class="card-heading">
				<span class="rule-id">{rule.id}</span>
				<Badge variant={severityVariant(rule.severity)}>{rule.severity}</Badge>
				<Badge variant={statusVariant(rule.status)}>{statusLabel(rule.status)}</Badge>
			</div>
			{#if rule.condition || (rule.evidence && rule.evidence.length > 0)}
				<button
					type="button"
					class="expand-toggle"
					aria-expanded={expanded}
					onclick={toggle}
				>
					{expanded ? 'Hide evidence' : 'Show evidence'}
				</button>
			{/if}
		</div>

		{#if rule.description}
			<p class="rule-description">{rule.description}</p>
		{/if}

		{#if rule.status === 'fail' && rule.failureReason}
			<Callout tone={calloutTone(rule.status)}>{rule.failureReason}</Callout>
		{/if}

		{#if expanded}
			<div class="card-evidence">
				{#if rule.condition}
					<CodeBlock code={rule.condition} language={rule.conditionLanguage ?? 'text'} />
				{/if}
				{#if rule.evidence && rule.evidence.length > 0}
					<table class="evidence-table">
						<thead>
							<tr>
								<th>Fact</th>
								<th>Value</th>
							</tr>
						</thead>
						<tbody>
							{#each rule.evidence as row (row.fact)}
								<tr>
									<td>{row.fact}</td>
									<td>{row.value}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				{/if}
			</div>
		{/if}
	{/if}
</div>

<style>
	.praxis-rule-card {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 14px;
		border-radius: 8px;
		border: 1px solid var(--color-border, #2d3140);
		background: var(--color-surface, #1a1d27);
	}

	.status-fail {
		border-color: color-mix(in srgb, var(--color-danger, #ef4444) 45%, var(--color-border, #2d3140));
	}

	.card-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.card-heading {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.rule-id {
		font-family: var(--font-mono, monospace);
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--color-text, #e2e5eb);
	}

	.expand-toggle {
		flex: 0 0 auto;
		background: none;
		border: none;
		color: var(--color-accent, #6366f1);
		font-size: 0.75rem;
		cursor: pointer;
		padding: 2px 4px;
	}

	.expand-toggle:hover {
		text-decoration: underline;
	}

	.rule-description {
		margin: 0;
		font-size: 0.8rem;
		color: var(--color-text-muted, #8b92a5);
	}

	.card-evidence {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.evidence-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.78rem;
	}

	.evidence-table th,
	.evidence-table td {
		text-align: left;
		padding: 4px 8px;
		border-bottom: 1px solid var(--color-border, #2d3140);
	}

	.evidence-table th {
		color: var(--color-text-muted, #8b92a5);
		font-weight: 600;
		text-transform: uppercase;
		font-size: 0.65rem;
		letter-spacing: 0.03em;
	}

	.evidence-table td {
		color: var(--color-text, #e2e5eb);
		font-family: var(--font-mono, monospace);
	}

	.skeleton {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.skeleton-row {
		height: 20px;
		border-radius: 6px;
		background: linear-gradient(
			90deg,
			var(--color-surface, #1a1d27) 25%,
			color-mix(in srgb, var(--color-surface, #1a1d27) 60%, var(--color-border, #2d3140)) 50%,
			var(--color-surface, #1a1d27) 75%
		);
		background-size: 200% 100%;
	}

	@media (prefers-reduced-motion: no-preference) {
		.skeleton-row {
			animation: shimmer 1.4s ease-in-out infinite;
		}
	}

	@keyframes shimmer {
		0% { background-position: 200% 0; }
		100% { background-position: -200% 0; }
	}
</style>
