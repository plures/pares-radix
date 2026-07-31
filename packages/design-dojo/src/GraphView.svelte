<!-- @component
  GraphView — ego-centric, space-adaptive graph navigation primitive (ADR-0032).

  Renders a focused node in the center surrounded by its edges, each terminating in a
  linked-neighbor stub. Selecting a stub re-centers it (host-mediated: fires
  onFocusChange, the host re-queries PluresDB and passes a fresh neighborhood — GraphView
  never walks the graph or owns persistent state, C-PLURES-003).

  Layout is "graph-flex": a constraint/space-budget pass (no physics engine) that grants
  the focus priority space + detail, distributes stubs radially weighted by container
  aspect ratio, and auto-summarizes each node's detail level (icon -> title ->
  title+keyFields -> full) from the box it is granted. Expanding one node only reflows
  neighbors that would be forced below `minNodeSize`; unaffected neighbors are untouched.
  High-degree foci collapse excess stubs into an "N more" affordance.

  Supports GUI (radial) and TUI (focus card + labeled edge list) token sets over the same
  data contract.
-->
<script lang="ts">
	import type { GraphViewProps, GraphNode, GraphEdge, DetailLevel } from './types-local.js';
	import {
		computeLayout,
		DEFAULT_MIN_NODE_SIZE,
		type NodePlacement,
	} from './graph-layout.js';

	let {
		neighborhood,
		onFocusChange,
		onExpand,
		onAction,
		detailFor,
		minNodeSize = DEFAULT_MIN_NODE_SIZE,
		zoom = 1,
		tui = false,
		class: className = '',
	}: GraphViewProps = $props();

	// --- Container space budget (ResizeObserver-driven; no breakpoint code) ---
	let containerEl = $state<HTMLDivElement | null>(null);
	let cw = $state(960);
	let ch = $state(600);

	$effect(() => {
		const el = containerEl;
		if (!el || typeof ResizeObserver === 'undefined') return;
		const ro = new ResizeObserver((entries) => {
			for (const e of entries) {
				const box = e.contentRect;
				if (box.width > 0) cw = box.width;
				if (box.height > 0) ch = box.height;
			}
		});
		ro.observe(el);
		return () => ro.disconnect();
	});

	// Which stub is locally expanded (visual only; fuller data is a host concern).
	let expandedId = $state<string | null>(null);

	const nodeById = $derived.by(() => {
		const m = new Map<string, GraphNode>();
		for (const n of neighborhood.nodes) m.set(n.id, n);
		return m;
	});

	const layout = $derived(
		computeLayout(neighborhood, {
			container: { width: cw, height: ch },
			zoom,
			minNodeSize,
			expandedId,
			detailFor,
		}),
	);

	const focusPlacement = $derived(layout.placements.find((p) => p.isFocus) ?? null);
	const stubPlacements = $derived(layout.placements.filter((p) => !p.isFocus));

	function placementNode(p: NodePlacement): GraphNode | undefined {
		return nodeById.get(p.nodeId);
	}

	// Edges that connect the focus to a rendered stub (for the radiating lines / TUI list).
	const renderedIds = $derived(new Set(layout.placements.map((p) => p.nodeId)));
	const focusEdges = $derived(
		neighborhood.edges.filter(
			(e) =>
				(e.from === neighborhood.focusId && renderedIds.has(e.to)) ||
				(e.to === neighborhood.focusId && renderedIds.has(e.from)),
		),
	);

	function edgeFor(stubId: string): GraphEdge | undefined {
		return neighborhood.edges.find(
			(e) =>
				(e.from === neighborhood.focusId && e.to === stubId) ||
				(e.to === neighborhood.focusId && e.from === stubId),
		);
	}

	function otherEndLabel(e: GraphEdge): string {
		const otherId = e.from === neighborhood.focusId ? e.to : e.from;
		return nodeById.get(otherId)?.label ?? otherId;
	}

	// --- Navigation (host-mediated: we never walk the graph ourselves) -------
	function focusStub(nodeId: string) {
		expandedId = null;
		onFocusChange?.(nodeId);
	}

	function toggleExpand(nodeId: string) {
		expandedId = expandedId === nodeId ? null : nodeId;
		if (expandedId) onExpand?.(nodeId);
	}

	function keyFields(node: GraphNode, limit = 3): [string, string][] {
		if (!node.fields) return [];
		return Object.entries(node.fields)
			.slice(0, limit)
			.map(([k, v]) => [k, formatValue(v)]);
	}

	function formatValue(v: unknown): string {
		if (v === null || v === undefined) return '';
		if (typeof v === 'object') return JSON.stringify(v);
		return String(v);
	}

	function initialsOf(node: GraphNode): string {
		return (node.label || node.id).trim().charAt(0).toUpperCase() || 'ΓÇó';
	}

	function showTitle(d: DetailLevel): boolean {
		return d !== 'icon';
	}
	function showFields(d: DetailLevel): boolean {
		return d === 'title+keyFields' || d === 'full';
	}
	function showActions(d: DetailLevel): boolean {
		return d === 'full';
	}
</script>

{#if tui}
	<!-- TUI token set: focus card + labeled edge list, same data contract. -->
	<div class="graphview-tui {className}" role="group" aria-label="Graph focus">
		{#if focusPlacement}
			{@const focus = placementNode(focusPlacement)}
			{#if focus}
				<div class="tui-focus">
					<div class="tui-focus-head">Γùë {focus.label}{#if focus.type}<span class="tui-type"> [{focus.type}]</span>{/if}</div>
					{#each keyFields(focus, 6) as [k, v] (k)}
						<div class="tui-field"><span class="tui-key">{k}:</span> {v}</div>
					{/each}
				</div>
			{/if}
		{/if}
		<ul class="tui-edges">
			{#each focusEdges as e (e.id)}
				{@const otherId = e.from === neighborhood.focusId ? e.to : e.from}
				<li>
					<button type="button" class="tui-edge" onclick={() => focusStub(otherId)}>
						Γö£ΓöÇ {e.label ?? 'ΓÇö'} ΓåÆ {otherEndLabel(e)}
					</button>
				</li>
			{/each}
			{#if layout.collapsedCount > 0}
				<li class="tui-more">ΓööΓöÇ ΓÇª {layout.collapsedCount} more</li>
			{/if}
		</ul>
	</div>
{:else}
	<!-- GUI token set: ego-centric radial layout. -->
	<div class="graphview {className}" bind:this={containerEl} role="group" aria-label="Graph neighborhood">
		<!-- Radiating edges (SVG under the nodes). -->
		<svg class="edges" width={cw} height={ch} aria-hidden="true">
			{#if focusPlacement}
				{#each stubPlacements as p (p.nodeId)}
					{@const e = edgeFor(p.nodeId)}
					<line
						x1={focusPlacement.x}
						y1={focusPlacement.y}
						x2={p.x}
						y2={p.y}
						class="edge-line"
						class:directed={e?.directed}
					/>
					{#if e?.label && (p.detail === 'title+keyFields' || p.detail === 'full')}
						<text
							class="edge-label"
							x={(focusPlacement.x + p.x) / 2}
							y={(focusPlacement.y + p.y) / 2}
						>{e.label}</text>
					{/if}
				{/each}
			{/if}
		</svg>

		<!-- Focus node (center; protected detail). -->
		{#if focusPlacement}
			{@const focus = placementNode(focusPlacement)}
			{#if focus}
				<div
					class="node focus detail-{focusPlacement.detail}"
					style="left:{focusPlacement.x}px; top:{focusPlacement.y}px; width:{focusPlacement.w}px; height:{focusPlacement.h}px;"
				>
					<div class="node-body">
						{#if showTitle(focusPlacement.detail)}
							<div class="node-title">{focus.label}</div>
							{#if focus.type}<div class="node-type">{focus.type}</div>{/if}
						{:else}
							<div class="node-icon" aria-label={focus.label}>{initialsOf(focus)}</div>
						{/if}
						{#if showFields(focusPlacement.detail)}
							<dl class="node-fields">
								{#each keyFields(focus) as [k, v] (k)}
									<div class="field-row"><dt>{k}</dt><dd>{v}</dd></div>
								{/each}
							</dl>
						{/if}
						{#if showActions(focusPlacement.detail) && focus.actions?.length}
							<div class="node-actions">
								{#each focus.actions as a (a.id)}
									<button type="button" class="node-action" onclick={() => onAction?.(focus.id, a.id)}>
										{#if a.icon}<span aria-hidden="true">{a.icon}</span>{/if}{a.label}
									</button>
								{/each}
							</div>
						{/if}
					</div>
				</div>
			{/if}
		{/if}

		<!-- Stub nodes (outer ring; demote first; expand independently). -->
		{#each stubPlacements as p (p.nodeId)}
			{@const node = placementNode(p)}
			{#if node}
				<div
					class="node stub detail-{p.detail}"
					class:reflowed={p.reflowed}
					class:expanded={expandedId === p.nodeId}
					style="left:{p.x}px; top:{p.y}px; width:{p.w}px; height:{p.h}px;"
				>
					<button
						type="button"
						class="node-body node-focus-btn"
						title={node.label}
						onclick={() => focusStub(node.id)}
					>
						{#if showTitle(p.detail)}
							<div class="node-title">{node.label}</div>
							{#if p.detail === 'full' && node.type}<div class="node-type">{node.type}</div>{/if}
						{:else}
							<div class="node-icon" aria-label={node.label}>{initialsOf(node)}</div>
						{/if}
						{#if showFields(p.detail)}
							<dl class="node-fields">
								{#each keyFields(node, 2) as [k, v] (k)}
									<div class="field-row"><dt>{k}</dt><dd>{v}</dd></div>
								{/each}
							</dl>
						{/if}
					</button>
					{#if p.detail !== 'icon'}
						<button
							type="button"
							class="expand-btn"
							aria-label={expandedId === p.nodeId ? `Collapse ${node.label}` : `Expand ${node.label}`}
							aria-expanded={expandedId === p.nodeId}
							onclick={() => toggleExpand(node.id)}
						>{expandedId === p.nodeId ? 'ΓêÆ' : '+'}</button>
					{/if}
				</div>
			{/if}
		{/each}

		<!-- High-degree "N more" affordance. -->
		{#if layout.collapsedCount > 0}
			<div class="more-affordance" title="{layout.collapsedCount} more neighbors">
				+{layout.collapsedCount} more
			</div>
		{/if}
	</div>
{/if}

<style>
	.graphview {
		position: relative;
		width: 100%;
		height: 100%;
		min-height: 320px;
		overflow: hidden;
		background: var(--color-bg, var(--color-surface));
		border: 1px solid var(--color-border);
		border-radius: 8px;
		container-type: size;
	}

	.edges {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}

	.edge-line {
		stroke: var(--color-border);
		stroke-width: 1.5;
	}

	.edge-line.directed {
		stroke: var(--color-accent, #6366f1);
		stroke-dasharray: 4 3;
	}

	.edge-label {
		fill: var(--color-text-muted);
		font-size: 0.7rem;
		text-anchor: middle;
		dominant-baseline: middle;
	}

	.node {
		position: absolute;
		transform: translate(-50%, -50%);
		box-sizing: border-box;
		display: flex;
		border-radius: 8px;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-text);
		transition: width 0.15s ease, height 0.15s ease, left 0.15s ease, top 0.15s ease;
		overflow: hidden;
	}

	.node.focus {
		border-color: var(--color-accent, #6366f1);
		box-shadow: 0 0 0 2px var(--color-accent-bg, rgba(99, 102, 241, 0.2));
		z-index: 2;
	}

	.node.stub {
		z-index: 1;
	}

	.node.stub.expanded {
		z-index: 3;
		border-color: var(--color-accent, #6366f1);
	}

	.node.stub.reflowed {
		opacity: 0.85;
	}

	.node-body {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 6px 8px;
		text-align: left;
	}

	.node-focus-btn {
		background: transparent;
		border: none;
		color: inherit;
		font: inherit;
		cursor: pointer;
		width: 100%;
		align-items: stretch;
	}

	.node-focus-btn:hover {
		background: var(--color-hover);
	}

	.node-icon {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		font-weight: 700;
		font-size: 1.1rem;
		color: var(--color-text-muted);
	}

	.node-title {
		font-weight: 600;
		font-size: 0.85rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.node-type {
		font-size: 0.7rem;
		color: var(--color-text-muted);
	}

	.node-fields {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
		font-size: 0.72rem;
	}

	.field-row {
		display: flex;
		gap: 6px;
		min-width: 0;
	}

	.field-row dt {
		color: var(--color-text-muted);
		flex-shrink: 0;
	}

	.field-row dd {
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.node-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		margin-top: auto;
	}

	.node-action {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		color: var(--color-text);
		border-radius: 5px;
		padding: 2px 8px;
		font-size: 0.72rem;
		cursor: pointer;
	}

	.node-action:hover {
		background: var(--color-hover);
	}

	.expand-btn {
		flex-shrink: 0;
		width: 20px;
		align-self: flex-start;
		background: transparent;
		border: none;
		border-left: 1px solid var(--color-border);
		color: var(--color-text-muted);
		cursor: pointer;
		font-size: 0.9rem;
		line-height: 1;
	}

	.expand-btn:hover {
		background: var(--color-hover);
		color: var(--color-text);
	}

	.more-affordance {
		position: absolute;
		right: 8px;
		bottom: 8px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		padding: 3px 10px;
		font-size: 0.75rem;
		color: var(--color-text-muted);
		z-index: 4;
	}

	/* --- TUI token set ---------------------------------------------------- */
	.graphview-tui {
		font-family: var(--font-mono, ui-monospace, monospace);
		color: var(--color-text);
		background: var(--color-bg, var(--color-surface));
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 10px 12px;
		font-size: 0.85rem;
	}

	.tui-focus-head {
		font-weight: 700;
		color: var(--color-accent, #6366f1);
		margin-bottom: 4px;
	}

	.tui-type {
		color: var(--color-text-muted);
		font-weight: 400;
	}

	.tui-field {
		color: var(--color-text-muted);
	}

	.tui-key {
		color: var(--color-text);
	}

	.tui-edges {
		list-style: none;
		margin: 8px 0 0;
		padding: 0;
	}

	.tui-edge {
		background: transparent;
		border: none;
		color: var(--color-text);
		font: inherit;
		cursor: pointer;
		padding: 2px 0;
		text-align: left;
		width: 100%;
	}

	.tui-edge:hover {
		color: var(--color-accent, #6366f1);
	}

	.tui-more {
		color: var(--color-text-muted);
		padding: 2px 0;
	}
</style>
