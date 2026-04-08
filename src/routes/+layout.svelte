<script lang="ts">
	import { page } from '$app/state';
	import { Sidebar, PluginContentArea, CommandPalette } from '@plures/design-dojo';
	import type { CommandItem } from '@plures/design-dojo';
	import { goto } from '$app/navigation';
	import { query, initPraxisFacts, toggleTheme, getTheme, emitFact } from '$lib/stores/praxis-svelte.js';
	import {
		createPluresDBAdapter,
		localStorageGraph,
		setSharedGraph,
		setSharedAdapter,
	} from '$lib/stores/plures-db-adapter.js';
	import { shellModule } from '$lib/praxis/shell.js';
	import { agensModule } from '$lib/praxis/agens.js';
	import {
		listenTauriEvents,
		tauriGetWindowState,
		tauriSetTrayMenu,
		isTauri,
	} from '$lib/platform/tauri.js';
	import { onMount } from 'svelte';
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	// Wire PluresDB adapter then initialise praxis facts once on mount.
	// The adapter must be set before initPraxisFacts so that:
	//   1. hydrateAll() restores persisted facts from PluresDB
	//   2. emitFact() persists any new facts immediately
	// onMount is used (not $effect) because this setup has no reactive
	// dependencies and must run exactly once.
	onMount(() => {
		const db = localStorageGraph();
		setSharedGraph(db);
		setSharedAdapter(
			createPluresDBAdapter({
				db,
				registry: [...shellModule.facts, ...agensModule.facts],
			}),
		);
		initPraxisFacts();

		// ── Tauri 2 integration ────────────────────────────────────────────────
		// Wire Tauri backend events → praxis facts (events not commands pattern).
		// All handlers are no-ops in the browser; isTauri() guards are advisory.
		let unlistenTauri: (() => void) | null = null;

		listenTauriEvents({
			// On app-booted: restore persisted window geometry from app.window fact.
			onAppBooted: async (_payload) => {
				emitFact('app.ready', { ready: true });
				// Seed window state from the Rust backend (actual geometry).
				const windowState = await tauriGetWindowState();
				if (windowState) {
					emitFact('app.window', windowState);
				}
			},
			// On window-state-changed: persist geometry via the praxis adapter.
			// rule.window-state fires; app.window (persist:true) is written to PluresDB.
			onWindowStateChanged: (state) => {
				emitFact('window.state.changed', state);
			},
			// On user-navigated (from tray click): route to the requested path.
			onUserNavigated: ({ path }) => {
				emitFact('user.navigated', { path });
				goto(path);
			},
		}).then((unlisten) => {
			unlistenTauri = unlisten;
		});

		return () => {
			unlistenTauri?.();
		};
	});

	// Reactive bindings via praxis query()
	let themeValue = $derived(
		(query<{ value: 'light' | 'dark' }>('theme.applied')?.value) ?? getTheme()
	);
	let navItems = $derived(
		(query<{ items: { href: string; label: string; icon?: string; badge?: number }[] }>('nav.visible')?.items) ?? []
	);

	// Sync tray menu whenever nav.visible changes (Tauri only).
	// Converts nav items to tray items via the tray.menu.requested praxis event.
	$effect(() => {
		if (!isTauri() || navItems.length === 0) return;
		const trayItems = navItems.map((item) => ({
			id: item.href.replace(/^\//, '').replace(/\//g, '-') || 'home',
			label: item.label,
			path: item.href,
		}));
		tauriSetTrayMenu(trayItems);
	});

	let sidebarCollapsed = $state(false);
	let paletteOpen = $state(false);

	// Built-in platform commands for the command palette
	let commands: CommandItem[] = $derived([
		{
			id: 'nav.home',
			label: 'Go to Dashboard',
			icon: '🏠',
			action: () => { goto('/'); },
		},
		{
			id: 'nav.settings',
			label: 'Go to Settings',
			icon: '⚙️',
			action: () => { goto('/settings'); },
		},
		{
			id: 'nav.help',
			label: 'Go to Help',
			icon: '❓',
			action: () => { goto('/help'); },
		},
		{
			id: 'theme.toggle',
			label: `Switch to ${themeValue === 'dark' ? 'Light' : 'Dark'} theme`,
			icon: themeValue === 'dark' ? '☀️' : '🌙',
			action: () => { toggleTheme(); },
		},
	]);

	// Global keyboard shortcut: Ctrl+K / Cmd+K opens the command palette
	$effect(() => {
		function handleKeydown(e: KeyboardEvent) {
			if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
				e.preventDefault();
				paletteOpen = true;
			}
		}
		window.addEventListener('keydown', handleKeydown);
		return () => window.removeEventListener('keydown', handleKeydown);
	});

	let statusItems = $derived([
		{ label: 'Theme', value: themeValue },
		{ label: 'Radix', value: 'v0.2.0' },
	]);
</script>

<div class="app" data-theme={themeValue}>
	<Sidebar
		items={navItems}
		currentPath={page.url.pathname}
		collapsed={sidebarCollapsed}
		onToggle={() => (sidebarCollapsed = !sidebarCollapsed)}
	/>

	<PluginContentArea
		theme={themeValue}
		onThemeToggle={toggleTheme}
		onSidebarToggle={() => (sidebarCollapsed = !sidebarCollapsed)}
		onCommandPaletteOpen={() => (paletteOpen = true)}
		{statusItems}
	>
		{@render children()}
	</PluginContentArea>

	<CommandPalette
		bind:open={paletteOpen}
		{commands}
		onClose={() => (paletteOpen = false)}
	/>
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

	/* Full-height flex row: sidebar (fixed) + content column (fills rest) */
	.app { display: flex; height: 100vh; overflow: hidden; }
</style>
