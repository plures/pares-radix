<script lang="ts">
	import { page } from '$app/state';
	import { getAllNavItems } from '$lib/platform/plugin-loader.js';
	import { theme } from '$lib/stores/theme.js';
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	let sidebarCollapsed = $state(false);
	let navItems = $derived(getAllNavItems());
</script>

<div class="app" data-theme={theme.value}>
	<aside class="sidebar" class:collapsed={sidebarCollapsed}>
		<nav class="sidebar-nav">
			{#each navItems as item}
				{@const active = page.url.pathname === item.href || (item.href !== '/' && page.url.pathname.startsWith(item.href + '/'))}
				<a href={item.href} class="nav-link" class:active>
					{#if item.icon}<span class="nav-icon">{item.icon}</span>{/if}
					{#if !sidebarCollapsed}<span class="nav-label">{item.label}</span>{/if}
				</a>
			{/each}
		</nav>
	</aside>

	<div class="content">
		<header class="topbar">
			<button class="btn-ghost" onclick={() => sidebarCollapsed = !sidebarCollapsed} aria-label="Toggle sidebar">
				{sidebarCollapsed ? '☰' : '◀'}
			</button>
			<div class="topbar-actions">
				<button class="btn-ghost" onclick={() => theme.toggle()} aria-label="Toggle theme">
					{theme.value === 'dark' ? '☀️' : '🌙'}
				</button>
			</div>
		</header>
		<main class="page">
			{@render children()}
		</main>
		<footer class="status-bar">
			<span class="status-item"><span class="status-key">Theme</span> <span class="status-val">{theme.value}</span></span>
			<span class="status-spacer"></span>
			<span class="status-item"><span class="status-key">Radix</span> <span class="status-val">v0.2.0</span></span>
		</footer>
	</div>
</div>

<style>
	:global(:root), :global([data-theme="light"]) {
		--color-bg: #f8f9fa;
		--color-surface: #ffffff;
		--color-border: #e2e5e9;
		--color-text: #1a1d21;
		--color-text-muted: #6b7280;
		--color-accent: #4f46e5;
		--color-accent-bg: rgba(79, 70, 229, 0.1);
		--color-hover: rgba(0, 0, 0, 0.04);
		--color-danger: #dc2626;
	}

	:global([data-theme="dark"]) {
		--color-bg: #0f1117;
		--color-surface: #1a1d27;
		--color-border: #2d3140;
		--color-text: #e2e5eb;
		--color-text-muted: #8b92a5;
		--color-accent: #6366f1;
		--color-accent-bg: rgba(99, 102, 241, 0.15);
		--color-hover: rgba(255, 255, 255, 0.05);
		--color-danger: #ef4444;
	}

	:global(*, *::before, *::after) { box-sizing: border-box; }

	:global(body) {
		margin: 0;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
		background: var(--color-bg);
		color: var(--color-text);
	}

	.app { display: flex; min-height: 100vh; }

	.content {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.topbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 4px 8px;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-surface);
		height: 44px;
	}

	.page { flex: 1; padding: 24px; overflow-y: auto; }

	.sidebar-nav { display: flex; flex-direction: column; gap: 2px; }

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
	.nav-icon { font-size: 1.1rem; width: 20px; text-align: center; }

	.sidebar {
		width: 220px;
		background: var(--color-surface);
		border-right: 1px solid var(--color-border);
		padding: 12px 8px;
		transition: width 0.2s ease;
		overflow: hidden;
		flex-shrink: 0;
	}

	.sidebar.collapsed { width: 52px; }

	.btn-ghost {
		background: none;
		border: none;
		cursor: pointer;
		padding: 6px 8px;
		border-radius: 6px;
		color: var(--color-text);
		font-size: 1rem;
		line-height: 1;
	}

	.btn-ghost:hover { background: var(--color-hover); }

	.status-bar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 12px;
		border-top: 1px solid var(--color-border);
		background: var(--color-surface);
		font-size: 0.75rem;
		color: var(--color-text-muted);
		height: 28px;
	}

	.status-item { display: flex; gap: 4px; }
	.status-key { opacity: 0.7; }
	.status-spacer { flex: 1; }
</style>
