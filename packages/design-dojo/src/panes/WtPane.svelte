<script lang="ts">
	/**
	 * WtPane — a titled, collapsible dock region (header + optional actions + body).
	 * Header chevron toggles collapse (aria-expanded); body is hidden when collapsed.
	 * `collapsed` is $bindable; oncollapse notifies the caller (who persists).
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		icon?: string;
		collapsed?: boolean;
		collapsible?: boolean;
		actions?: Snippet;
		children: Snippet;
		oncollapse?: (collapsed: boolean) => void;
	}

	let {
		title,
		icon,
		collapsed = $bindable(false),
		collapsible = true,
		actions,
		children,
		oncollapse
	}: Props = $props();

	function toggle() {
		if (!collapsible) return;
		collapsed = !collapsed;
		oncollapse?.(collapsed);
	}
</script>

<section class="wt-pane" class:collapsed>
	<header class="wt-pane-header">
		{#if collapsible}
			<button
				type="button"
				class="wt-pane-chevron"
				aria-expanded={!collapsed}
				aria-label={collapsed ? `Expand ${title}` : `Collapse ${title}`}
				onclick={toggle}
			>
				<span class="chev" class:open={!collapsed} aria-hidden="true">▸</span>
			</button>
		{/if}
		{#if icon}
			<span class="wt-pane-icon" aria-hidden="true">{icon}</span>
		{/if}
		<span class="wt-pane-title">{title}</span>
		{#if actions}
			<span class="wt-pane-actions">{@render actions()}</span>
		{/if}
	</header>

	{#if !collapsed}
		<div class="wt-pane-body">
			{@render children()}
		</div>
	{/if}
</section>

<style>
	.wt-pane {
		display: flex;
		flex-direction: column;
		min-height: 0;
		height: 100%;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		overflow: hidden;
	}
	.wt-pane-header {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 8px;
		background: var(--color-surface-alt, var(--color-surface));
		border-bottom: 1px solid var(--color-border);
		font-size: 0.82rem;
		user-select: none;
	}
	.wt-pane.collapsed .wt-pane-header {
		border-bottom: none;
	}
	.wt-pane-chevron {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		padding: 0;
		border: none;
		background: transparent;
		color: var(--color-text);
		cursor: pointer;
		border-radius: 3px;
	}
	.wt-pane-chevron:hover {
		background: var(--color-border);
	}
	.chev {
		display: inline-block;
		transition: transform 0.12s ease;
	}
	.chev.open {
		transform: rotate(90deg);
	}
	.wt-pane-icon {
		font-size: 0.9rem;
	}
	.wt-pane-title {
		font-weight: 600;
		color: var(--color-text);
		text-transform: uppercase;
		letter-spacing: 0.03em;
		font-size: 0.74rem;
	}
	.wt-pane-actions {
		margin-left: auto;
		display: inline-flex;
		gap: 4px;
	}
	.wt-pane-body {
		flex: 1 1 auto;
		min-height: 0;
		overflow: auto;
		padding: 8px;
		color: var(--color-text);
	}
</style>
