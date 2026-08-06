<!--
  EpicStatusBoard — status-hierarchy card renderer for epic/task registry data
  (id, status, priority, tier, parent_epic_id chain, blocked_on, next_action).

  Deliberately NOT a Kanban board: epic-registry-shaped entries carry free-text
  `next_action` narratives + a parent/blocked-on hierarchy that a column-per-status
  card board renders poorly. This renders a flat, filterable list of cards instead,
  each card showing its full status/priority/tier/hierarchy/next-action at a glance.
-->
<script lang="ts">
	import type { EpicStatusBoardProps, EpicEntry } from './types-local.js';

	let { entries, onSelect, class: className = '' }: EpicStatusBoardProps = $props();

	let statusFilter = $state<string>('all');
	let loading = $derived(entries === undefined);

	const STATUS_LABELS: Record<string, string> = {
		in_progress: 'In progress',
		awaiting_approval: 'Awaiting approval',
		pending_pr: 'Pending PR',
		assistance_required: 'Assistance required',
		blocked: 'Blocked',
		complete: 'Complete',
	};

	const PRIORITY_LABELS: Record<string, string> = {
		p0: 'P0',
		p1: 'P1',
		p2: 'P2',
	};

	let resolvedEntries = $derived(entries ?? []);

	let availableStatuses = $derived(
		Array.from(new Set(resolvedEntries.map((e) => e.status))).sort()
	);

	let visibleEntries = $derived(
		statusFilter === 'all'
			? resolvedEntries
			: resolvedEntries.filter((e) => e.status === statusFilter)
	);

	function parentChain(entry: EpicEntry, byId: Map<string, EpicEntry>): EpicEntry[] {
		const chain: EpicEntry[] = [];
		let current = entry.parentEpicId ? byId.get(entry.parentEpicId) : undefined;
		let guard = 0;
		while (current && guard < 20) {
			chain.unshift(current);
			current = current.parentEpicId ? byId.get(current.parentEpicId) : undefined;
			guard++;
		}
		return chain;
	}

	let entriesById = $derived(new Map(resolvedEntries.map((e) => [e.id, e])));

	function statusLabel(status: string): string {
		return STATUS_LABELS[status] ?? status;
	}

	function priorityLabel(priority: string): string {
		return PRIORITY_LABELS[priority] ?? priority.toUpperCase();
	}
</script>

<div class="epic-status-board {className}">
	<div class="board-toolbar">
		<label class="filter-label" for="epic-status-filter">Filter by status</label>
		<select
			id="epic-status-filter"
			class="status-filter"
			bind:value={statusFilter}
			disabled={loading || resolvedEntries.length === 0}
		>
			<option value="all">All statuses ({resolvedEntries.length})</option>
			{#each availableStatuses as status (status)}
				<option value={status}>
					{statusLabel(status)} ({resolvedEntries.filter((e) => e.status === status).length})
				</option>
			{/each}
		</select>
	</div>

	{#if loading}
		<ul class="epic-list" aria-busy="true" aria-label="Loading epics">
			{#each { length: 3 } as _, i (i)}
				<li class="epic-card skeleton" aria-hidden="true">
					<div class="skeleton-line w-40"></div>
					<div class="skeleton-line w-70"></div>
					<div class="skeleton-line w-90"></div>
				</li>
			{/each}
		</ul>
	{:else if resolvedEntries.length === 0}
		<div class="empty-state">
			<p>No epics registered yet.</p>
			<p class="muted">Epics appear here once they're added to the registry.</p>
		</div>
	{:else if visibleEntries.length === 0}
		<div class="empty-state">
			<p>No epics match the "{statusLabel(statusFilter)}" filter.</p>
			<button type="button" class="reset-filter" onclick={() => (statusFilter = 'all')}>
				Clear filter
			</button>
		</div>
	{:else}
		<ul class="epic-list">
			{#each visibleEntries as entry (entry.id)}
				{@const chain = parentChain(entry, entriesById)}
				<li class="epic-card status-{entry.status}">
					<button
						type="button"
						class="epic-card-inner"
						onclick={() => onSelect?.(entry)}
						disabled={!onSelect}
					>
						{#if chain.length > 0}
							<nav class="breadcrumb" aria-label="Epic hierarchy">
								{#each chain as ancestor, i (ancestor.id)}
									<span class="crumb">{ancestor.id}</span>
									<span class="crumb-sep" aria-hidden="true">/</span>
								{/each}
							</nav>
						{/if}

						<div class="epic-card-header">
							<span class="epic-id">{entry.id}</span>
							<span class="badge tier-{entry.tier}">{entry.tier}</span>
							<span class="badge priority-{entry.priority}">{priorityLabel(entry.priority)}</span>
							<span class="badge status-badge status-{entry.status}">{statusLabel(entry.status)}</span>
						</div>

						{#if entry.blockedOn && entry.blockedOn.length > 0}
							<p class="blocked-on">
								<span class="blocked-on-label">Blocked on:</span>
								{entry.blockedOn.join(', ')}
							</p>
						{/if}

						{#if entry.nextAction}
							<p class="next-action">{entry.nextAction}</p>
						{/if}

						{#if entry.error}
							<p class="epic-error" role="alert">{entry.error}</p>
						{/if}
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.epic-status-board {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.board-toolbar {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.filter-label {
		font-size: 0.8rem;
		color: var(--color-text-muted, #8b92a5);
	}

	.status-filter {
		padding: 5px 10px;
		border-radius: 6px;
		border: 1px solid var(--color-border, #2d3140);
		background: var(--color-surface, #1a1d27);
		color: var(--color-text, #e2e5eb);
		font-size: 0.85rem;
	}

	.status-filter:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.epic-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: 1fr;
		gap: 10px;
	}

	@container (min-width: 640px) {
		.epic-list {
			grid-template-columns: repeat(2, 1fr);
		}
	}

	.epic-status-board {
		container-type: inline-size;
	}

	.epic-card {
		border: 1px solid var(--color-border, #2d3140);
		border-radius: 8px;
		background: var(--color-surface, #1a1d27);
		overflow: hidden;
	}

	.epic-card-inner {
		width: 100%;
		text-align: left;
		background: transparent;
		border: none;
		color: inherit;
		font: inherit;
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 6px;
		cursor: pointer;
	}

	.epic-card-inner:disabled {
		cursor: default;
	}

	.epic-card-inner:focus-visible {
		outline: 2px solid var(--color-accent, #6366f1);
		outline-offset: -2px;
	}

	.epic-card-inner:hover:not(:disabled) {
		background: var(--color-hover, rgba(255, 255, 255, 0.03));
	}

	.status-blocked,
	.status-assistance_required {
		border-color: var(--color-danger, #ef4444);
	}

	.status-in_progress {
		border-color: var(--color-accent, #6366f1);
	}

	.status-complete {
		border-color: var(--color-success, #22c55e);
	}

	.breadcrumb {
		font-size: 0.7rem;
		color: var(--color-text-muted, #8b92a5);
		display: flex;
		flex-wrap: wrap;
		gap: 2px;
	}

	.crumb-sep {
		margin: 0 2px;
	}

	.epic-card-header {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}

	.epic-id {
		font-weight: 700;
		font-size: 0.9rem;
		color: var(--color-text, #e2e5eb);
	}

	.badge {
		font-size: 0.68rem;
		font-weight: 600;
		padding: 2px 7px;
		border-radius: 10px;
		text-transform: uppercase;
		letter-spacing: 0.02em;
		background: var(--color-hover, rgba(255, 255, 255, 0.06));
		color: var(--color-text-muted, #8b92a5);
	}

	.priority-p0 {
		background: rgba(239, 68, 68, 0.15);
		color: var(--color-danger, #ef4444);
	}

	.priority-p1 {
		background: rgba(234, 179, 8, 0.15);
		color: #eab308;
	}

	.status-badge.status-in_progress {
		background: rgba(99, 102, 241, 0.15);
		color: var(--color-accent, #6366f1);
	}

	.status-badge.status-complete {
		background: rgba(34, 197, 94, 0.15);
		color: var(--color-success, #22c55e);
	}

	.status-badge.status-blocked,
	.status-badge.status-assistance_required {
		background: rgba(239, 68, 68, 0.15);
		color: var(--color-danger, #ef4444);
	}

	.blocked-on {
		margin: 0;
		font-size: 0.78rem;
		color: var(--color-danger, #ef4444);
	}

	.blocked-on-label {
		font-weight: 600;
	}

	.next-action {
		margin: 0;
		font-size: 0.8rem;
		color: var(--color-text-muted, #8b92a5);
		line-height: 1.4;
	}

	.epic-error {
		margin: 0;
		font-size: 0.78rem;
		color: var(--color-danger, #ef4444);
		font-weight: 500;
	}

	.empty-state {
		text-align: center;
		padding: 40px 16px;
		color: var(--color-text-muted, #8b92a5);
	}

	.muted {
		font-size: 0.85rem;
		opacity: 0.7;
	}

	.reset-filter {
		margin-top: 8px;
		padding: 5px 14px;
		border-radius: 6px;
		border: 1px solid var(--color-border, #2d3140);
		background: transparent;
		color: var(--color-text, #e2e5eb);
		cursor: pointer;
		font-size: 0.8rem;
	}

	.reset-filter:hover {
		background: var(--color-hover, rgba(255, 255, 255, 0.05));
	}

	.reset-filter:focus-visible {
		outline: 2px solid var(--color-accent, #6366f1);
		outline-offset: 2px;
	}

	.skeleton {
		pointer-events: none;
	}

	.skeleton-line {
		height: 10px;
		border-radius: 4px;
		background: var(--color-border, #2d3140);
		margin: 8px 14px;
	}

	@media (prefers-reduced-motion: no-preference) {
		.skeleton-line {
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
			0% { background-position: 200% 0; }
			100% { background-position: -200% 0; }
		}
	}

	.w-40 { width: 40%; }
	.w-70 { width: 70%; }
	.w-90 { width: 90%; }
</style>
