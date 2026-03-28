<script lang="ts">
	import { page } from '$app/state';
	import type { NavItem } from '$lib/types/plugin.js';

	interface Props {
		items: NavItem[];
		collapsed: boolean;
		onToggle: () => void;
	}

	let { items, collapsed, onToggle }: Props = $props();
</script>

<aside class="sidebar" class:collapsed>
	<div class="sidebar-header">
		<a href="/" class="logo">
			{#if !collapsed}<span class="logo-text">Radix</span>{/if}
			<span class="logo-icon">⚡</span>
		</a>
		<button class="toggle-btn" onclick={onToggle} aria-label="Toggle sidebar">
			{collapsed ? '→' : '←'}
		</button>
	</div>

	<nav class="sidebar-nav">
		{#each items as item}
			{@const active = page.url.pathname === item.href || page.url.pathname.startsWith(item.href + '/')}
			<a
				href={item.href}
				class="nav-item"
				class:active
				title={collapsed ? item.label : undefined}
			>
				<span class="nav-icon">{item.icon}</span>
				{#if !collapsed}
					<span class="nav-label">{item.label}</span>
					{#if item.badge}
						{@const count = item.badge()}
						{#if count > 0}
							<span class="badge">{count}</span>
						{/if}
					{/if}
				{/if}
			</a>
			{#if item.children && !collapsed}
				{#each item.children as child}
					{@const childActive = page.url.pathname === child.href}
					<a href={child.href} class="nav-item child" class:active={childActive}>
						<span class="nav-icon">{child.icon}</span>
						<span class="nav-label">{child.label}</span>
					</a>
				{/each}
			{/if}
		{/each}
	</nav>

	<div class="sidebar-footer">
		<a href="/settings" class="nav-item" class:active={page.url.pathname === '/settings'}>
			<span class="nav-icon">⚙️</span>
			{#if !collapsed}<span class="nav-label">Settings</span>{/if}
		</a>
		<a href="/help" class="nav-item" class:active={page.url.pathname === '/help'}>
			<span class="nav-icon">❓</span>
			{#if !collapsed}<span class="nav-label">Help</span>{/if}
		</a>
	</div>
</aside>

<style>
	.sidebar {
		display: flex;
		flex-direction: column;
		width: 240px;
		height: 100vh;
		background: var(--color-surface);
		border-right: 1px solid var(--color-border);
		transition: width 0.2s ease;
		overflow: hidden;
		flex-shrink: 0;
	}

	.sidebar.collapsed {
		width: 56px;
	}

	.sidebar-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px;
		border-bottom: 1px solid var(--color-border);
	}

	.logo {
		display: flex;
		align-items: center;
		gap: 8px;
		text-decoration: none;
		color: var(--color-text);
		font-weight: 700;
		font-size: 1.1rem;
	}

	.toggle-btn {
		background: none;
		border: none;
		color: var(--color-text-muted);
		cursor: pointer;
		padding: 4px 8px;
		border-radius: 4px;
		font-size: 14px;
	}

	.toggle-btn:hover {
		background: var(--color-hover);
	}

	.sidebar-nav {
		flex: 1;
		overflow-y: auto;
		padding: 8px;
	}

	.nav-item {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 12px;
		border-radius: 6px;
		text-decoration: none;
		color: var(--color-text-muted);
		font-size: 0.9rem;
		transition: all 0.15s ease;
		white-space: nowrap;
	}

	.nav-item:hover {
		background: var(--color-hover);
		color: var(--color-text);
	}

	.nav-item.active {
		background: var(--color-accent-bg);
		color: var(--color-accent);
		font-weight: 500;
	}

	.nav-item.child {
		padding-left: 36px;
		font-size: 0.85rem;
	}

	.nav-icon {
		font-size: 1.1rem;
		flex-shrink: 0;
		width: 24px;
		text-align: center;
	}

	.badge {
		margin-left: auto;
		background: var(--color-accent);
		color: white;
		border-radius: 10px;
		padding: 1px 7px;
		font-size: 0.75rem;
		font-weight: 600;
	}

	.sidebar-footer {
		padding: 8px;
		border-top: 1px solid var(--color-border);
	}

	@media (max-width: 768px) {
		.sidebar {
			position: fixed;
			z-index: 100;
			left: 0;
			top: 0;
		}

		.sidebar.collapsed {
			width: 0;
			border: none;
		}
	}
</style>
