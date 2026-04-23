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
	import { designModule, buildSchemaRegistry } from '$lib/praxis/design.js';
	import { registerForHotReload } from '$lib/praxis/hot-reload.js';
	import { detectRenderMode, renderModeClass, tuiCssOverrides, type RenderMode } from '$lib/platform/render-mode.js';
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
				registry: [...shellModule.facts, ...agensModule.facts, ...designModule.facts],
			}),
		);
		initPraxisFacts();

		// Initialize the design mode schema registry from all loaded praxis modules
		const schemas = buildSchemaRegistry(shellModule, agensModule, designModule);
		emitFact('design.schema.registry', schemas);

		// Register modules for hot-reload
		registerForHotReload(shellModule);
		registerForHotReload(agensModule);
		registerForHotReload(designModule);

		// ── Tauri 2 integration ────────────────────────────────────────────────
		// Wire Tauri backend events → praxis facts (events not commands pattern).
		// All handlers are no-ops in the browser; isTauri() guards are advisory.
		// Store the promise so cleanup awaits resolution even if component unmounts
		// before listenTauriEvents resolves.
		const unlistenPromise = listenTauriEvents({
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
		});

		return () => {
			// Await the promise before calling unlisten to handle the case where
			// the component unmounts before the async listeners have resolved.
			// Swallow setup failures here so early unmounts do not surface
			// unhandled rejections from listenTauriEvents().
			unlistenPromise
				.then((unlisten) => unlisten())
				.catch(() => {});
		};
	});

	// Reactive bindings via praxis query()
	let themeValue = $derived(
		(query<{ value: 'light' | 'dark' }>('theme.applied')?.value) ?? getTheme()
	);
	let navItems = $derived(
		(query<{ items: { href: string; label: string; icon?: string; badge?: number }[] }>('nav.visible')?.items) ?? []
	);
	let designModeActive = $derived(
		(query<{ active: boolean }>('design.mode.active')?.active) ?? false
	);

	// Sync tray menu whenever nav.visible changes (Tauri only).
	// Item hrefs are used directly as IDs so the Rust on_menu_event handler
	// can emit the path without reconstruction (fixes path fidelity for nested routes).
	$effect(() => {
		if (!isTauri() || navItems.length === 0) return;
		const trayItems = navItems.map((item) => ({
			id: item.href,
			label: item.label,
			path: item.href,
		}));
		void tauriSetTrayMenu(trayItems).catch((error) => {
			console.error('Failed to sync tray menu', error);
		});
	});

	let sidebarCollapsed = $state(false);
	let paletteOpen = $state(false);
	let renderMode = $state<RenderMode>(detectRenderMode());

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
		{
			id: 'design.mode.toggle',
			label: designModeActive ? 'Exit Design Mode' : 'Enter Design Mode',
			icon: designModeActive ? '🔒' : '🎨',
			action: () => { emitFact('design.mode.active', { active: !designModeActive }); },
		},
		{
			id: 'render.mode.gui',
			label: 'Render: GUI Mode',
			icon: '🖥️',
			action: () => { renderMode = 'gui'; emitFact('render.mode', { mode: 'gui' }); },
		},
		{
			id: 'render.mode.tui',
			label: 'Render: TUI Mode (terminal aesthetics)',
			icon: '⌨️',
			action: () => { renderMode = 'tui-css'; emitFact('render.mode', { mode: 'tui-css' }); },
		},
	]);

	// Global keyboard shortcuts
	$effect(() => {
		function handleKeydown(e: KeyboardEvent) {
			// Ctrl+K / Cmd+K → command palette
			if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
				e.preventDefault();
				paletteOpen = true;
			}
			// Ctrl+Shift+D / Cmd+Shift+D → toggle design mode
			if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'D') {
				e.preventDefault();
				emitFact('design.mode.active', { active: !designModeActive });
			}
		}
		window.addEventListener('keydown', handleKeydown);
		return () => window.removeEventListener('keydown', handleKeydown);
	});

	let statusItems = $derived([
		{ label: 'Theme', value: themeValue },
		{ label: 'Render', value: renderMode === 'gui' ? 'GUI' : renderMode === 'tui-css' ? '⌨️ TUI' : '📟 Native' },
		{ label: 'Radix', value: 'v0.2.0' },
		...(designModeActive ? [{ label: 'Design', value: '🎨 Active' }] : []),
	]);
</script>

<div class="app {renderModeClass(renderMode)}" data-theme={themeValue}>
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

	{#if renderMode === 'tui-css'}
		{@html `<style>${tuiCssOverrides}</style>`}
	{/if}
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
