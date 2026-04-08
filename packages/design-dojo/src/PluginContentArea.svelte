<script lang="ts">
	import type { PluginContentAreaProps } from './types.js';
	import StatusBar from './StatusBar.svelte';

	let {
		theme = 'dark',
		onThemeToggle,
		onSidebarToggle,
		onCommandPaletteOpen,
		statusItems = [],
		children
	}: PluginContentAreaProps = $props();
</script>

<div class="content">
	<header class="topbar" aria-label="Topbar">
		<div class="topbar-start">
			<button
				class="topbar-btn"
				onclick={onSidebarToggle}
				aria-label="Toggle sidebar"
			>
				☰
			</button>
		</div>

		<div class="topbar-actions">
			{#if onCommandPaletteOpen}
				<button
					class="topbar-btn palette-trigger"
					onclick={onCommandPaletteOpen}
					aria-label="Open command palette"
					aria-keyshortcuts="Control+K Meta+K"
				>
					<span aria-hidden="true">⌘K</span>
				</button>
			{/if}

			<button
				class="topbar-btn"
				onclick={onThemeToggle}
				aria-label="Toggle theme"
			>
				{theme === 'dark' ? '☀️' : '🌙'}
			</button>
		</div>
	</header>

	<main class="page" id="main-content">
		{@render children()}
	</main>

	<StatusBar items={statusItems} />
</div>

<style>
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

	.topbar-start { display: flex; align-items: center; gap: 4px; }
	.topbar-actions { display: flex; align-items: center; gap: 4px; }

	.topbar-btn {
		background: transparent;
		border: none;
		cursor: pointer;
		padding: 6px 8px;
		border-radius: 6px;
		color: var(--color-text-muted);
		font-size: 1rem;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		transition: background 0.12s, color 0.12s;
	}

	.topbar-btn:hover { background: var(--color-hover); color: var(--color-text); }

	.palette-trigger {
		font-size: 0.8rem;
		font-weight: 500;
		border: 1px solid var(--color-border);
		padding: 4px 10px;
	}

	.page { flex: 1; padding: 24px; overflow-y: auto; }
</style>
