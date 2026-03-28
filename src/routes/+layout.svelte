<script lang="ts">
	import Sidebar from '$lib/components/Sidebar.svelte';
	import { getAllNavItems } from '$lib/platform/plugin-loader.js';
	import { theme } from '$lib/stores/theme.js';
	import { browser } from '$app/environment';
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	let navItems = $derived(getAllNavItems());

	// ── Responsive breakpoints ───────────────────────────────────────────────
	// Default to desktop width for SSR and initial client render; hydrates to actual width on the client.
	let windowWidth = $state(1280);

	$effect(() => {
		if (!browser) return;
		const onResize = () => { windowWidth = window.innerWidth; };
		window.addEventListener('resize', onResize, { passive: true });
		return () => window.removeEventListener('resize', onResize);
	});

	let isMobile = $derived(windowWidth < 768);
	let isTablet = $derived(windowWidth >= 768 && windowWidth < 1024);

	// ── Sidebar state ────────────────────────────────────────────────────────
	// Desktop: user-controlled toggle between full (240 px) and icon (56 px).
	let desktopCollapsed = $state(false);
	// Mobile: whether the full-width overlay is visible.
	let mobileOpen = $state(false);

	// Collapsed prop passed to <Sidebar>:
	//   mobile  → hidden when overlay closed, visible when open
	//   tablet  → always icon mode (auto-collapsed)
	//   desktop → follows user preference
	let sidebarCollapsed = $derived(
		isMobile ? !mobileOpen : (isTablet || desktopCollapsed)
	);

	function toggleSidebar() {
		if (isMobile) {
			mobileOpen = !mobileOpen;
		} else if (!isTablet) {
			// Tablet is always icon mode — nothing to toggle.
			desktopCollapsed = !desktopCollapsed;
		}
	}

	function closeMobileOverlay() {
		mobileOpen = false;
	}

	// ── Keyboard shortcut: Ctrl+/ or ⌘+/ toggles sidebar ───────────────────
	$effect(() => {
		if (!browser) return;
		function onKeyDown(e: KeyboardEvent) {
			if ((e.metaKey || e.ctrlKey) && e.key === '/') {
				e.preventDefault();
				toggleSidebar();
			}
		}
		window.addEventListener('keydown', onKeyDown);
		return () => window.removeEventListener('keydown', onKeyDown);
	});
</script>

<div class="app" data-theme={theme.value}>
	{#if isMobile && mobileOpen}
		<!-- Backdrop closes the mobile overlay when tapped outside the sidebar -->
		<div
			class="mobile-backdrop"
			onclick={closeMobileOverlay}
			aria-hidden="true"
		></div>
	{/if}

	<Sidebar
		items={navItems}
		collapsed={sidebarCollapsed}
		onToggle={toggleSidebar}
	/>

	<main class="content">
		<header class="topbar">
			<!-- Hamburger shown only on mobile; sidebar has its own toggle on desktop -->
			<button
				class="mobile-menu"
				onclick={toggleSidebar}
				aria-label="Open navigation menu"
				aria-expanded={mobileOpen}
				aria-controls="sidebar"
			>☰</button>
			<div class="topbar-actions">
				<button class="theme-toggle" onclick={() => theme.toggle()} aria-label="Toggle theme">
					{theme.value === 'dark' ? '☀️' : '🌙'}
				</button>
			</div>
		</header>
		<div class="page">
			{@render children()}
		</div>
	</main>
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

	:global(*, *::before, *::after) {
		box-sizing: border-box;
	}

	:global(body) {
		margin: 0;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
		background: var(--color-bg);
		color: var(--color-text);
	}

	.app {
		display: flex;
		min-height: 100vh;
	}

	/* Darkened overlay behind the mobile sidebar drawer */
	.mobile-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		z-index: 99;
	}

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
		padding: 8px 16px;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-surface);
		height: 48px;
	}

	/* Hamburger shown only on mobile; desktop uses the sidebar's own toggle */
	.mobile-menu {
		display: none;
		background: none;
		border: none;
		font-size: 1.2rem;
		cursor: pointer;
		color: var(--color-text);
		padding: 4px 8px;
	}

	.theme-toggle {
		background: none;
		border: none;
		font-size: 1.1rem;
		cursor: pointer;
		padding: 4px 8px;
		border-radius: 4px;
	}

	.theme-toggle:hover {
		background: var(--color-hover);
	}

	.page {
		flex: 1;
		padding: 24px;
		overflow-y: auto;
	}

	@media (max-width: 767px) {
		.mobile-menu {
			display: block;
		}
	}
</style>
