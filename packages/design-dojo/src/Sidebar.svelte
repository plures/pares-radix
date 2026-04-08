<script lang="ts">
	import type { SidebarProps } from './types.js';

	let { items, currentPath, collapsed = false, onToggle }: SidebarProps = $props();

	function isActive(href: string): boolean {
		return currentPath === href || (href !== '/' && currentPath.startsWith(href + '/'));
	}
</script>

<aside class="sidebar" class:collapsed aria-label="Main navigation">
	<nav class="sidebar-nav">
		<button
			class="toggle-btn"
			onclick={onToggle}
			aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
			aria-expanded={!collapsed}
		>
			{collapsed ? '☰' : '◀'}
		</button>

		{#each items as item (item.href)}
			{@const active = isActive(item.href)}
			<a
				href={item.href}
				class="nav-link"
				class:active
				aria-current={active ? 'page' : undefined}
				title={collapsed ? item.label : undefined}
			>
				{#if item.icon}
					<span class="nav-icon" aria-hidden="true">{item.icon}</span>
				{/if}
				{#if !collapsed}
					<span class="nav-label">{item.label}</span>
					{#if item.badge}
						<span class="nav-badge" aria-label={`${item.badge} unread`}>{item.badge}</span>
					{/if}
				{/if}
			</a>
		{/each}
	</nav>
</aside>

<style>
	.sidebar {
		width: 220px;
		flex-shrink: 0;
		background: var(--color-surface);
		border-right: 1px solid var(--color-border);
		padding: 12px 8px;
		display: flex;
		flex-direction: column;
		transition: width 0.15s ease;
	}

	.sidebar.collapsed { width: 52px; }

	.sidebar-nav { display: flex; flex-direction: column; gap: 2px; }

	.toggle-btn {
		background: transparent;
		border: none;
		cursor: pointer;
		padding: 6px 8px;
		border-radius: 6px;
		color: var(--color-text-muted);
		font-size: 1rem;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		margin-bottom: 8px;
		transition: background 0.12s, color 0.12s;
	}

	.toggle-btn:hover { background: var(--color-hover); color: var(--color-text); }

	.nav-link {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 12px;
		border-radius: 6px;
		color: var(--color-text-muted);
		text-decoration: none;
		font-size: 0.875rem;
		transition: background 0.12s, color 0.12s;
	}

	.nav-link:hover { background: var(--color-hover); color: var(--color-text); }
	.nav-link.active { background: var(--color-accent-bg); color: var(--color-accent); font-weight: 500; }
	.nav-icon { font-size: 1.1rem; width: 20px; text-align: center; flex-shrink: 0; }
	.nav-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.nav-badge {
		background: var(--color-accent);
		color: #fff;
		font-size: 0.7rem;
		font-weight: 700;
		border-radius: 10px;
		padding: 1px 6px;
		min-width: 18px;
		text-align: center;
	}
</style>
