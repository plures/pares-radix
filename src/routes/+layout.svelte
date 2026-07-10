<script lang="ts">
	import { page } from '$app/state';
	import { Box, Sidebar, PluginContentArea, CommandPalette } from '@plures/design-dojo';
	import Breadcrumbs from '$lib/components/Breadcrumbs.svelte';
	import type { CommandItem } from '@plures/design-dojo';
	import { goto } from '$app/navigation';
	import { query, initPraxisFacts, seedNavItems, toggleTheme, getTheme, emitFact } from '$lib/stores/praxis-svelte.svelte.js';
	import {
		createPluresDBAdapter,
		getSharedGraph,
		setSharedGraph,
		setSharedAdapter,
	} from '$lib/stores/plures-db-adapter.js';
	import { activateAll, registerPlugin } from '$lib/platform/plugin-loader.js';
	import { createPluginContext } from '$lib/platform/plugin-context.js';
	import { agensPlugin } from '$lib/plugins/agens/index.js';
	import { shellModule } from '$lib/praxis/shell.js';
	import { agensModule } from '$lib/praxis/agens.js';
	import { designModule, buildSchemaRegistry } from '$lib/praxis/design.js';
	import { operationsModule, wireOperationsScene } from '$lib/praxis/operations.js';
	import { adminModule, wireAdminScene, isPluginEnabled, shouldActivateOnStartup } from '$lib/praxis/admin.js';
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
		const db = getSharedGraph();
		setSharedGraph(db);
		setSharedAdapter(
			createPluresDBAdapter({
				db,
				registry: [
					...shellModule.facts,
					...agensModule.facts,
					...designModule.facts,
					...operationsModule.facts,
					...adminModule.facts,
				],
			}),
		);
		initPraxisFacts();

		// Register the agens agent-type plugin before activation so its nav item
		// (💬 Agens → /agent) flows through getAllNavItems() → toSidebarItems().
		// registerPlugin is idempotent (dedupes by id), so a hot-reload re-run is safe.
		registerPlugin(agensPlugin);

		// Seed the Operations-as-Intent demo scene (real fleet + constraint-checked
		// state) through the sanctioned emitFact path. Idempotent + hydration-safe:
		// wireOperationsScene no-ops if the fleet fact was already restored from
		// PluresDB, so a restart keeps operator-modified state.
		wireOperationsScene(emitFact, (factId) => query(factId));

		// Seed the Admin Console scene (feature flags + audit log) through the
		// sanctioned emitFact path; idempotent + hydration-safe so operator toggles
		// survive a restart. Health/readiness are derived live by the route on mount.
		wireAdminScene(emitFact, (factId) => query(factId));

		// Activate all registered plugins now that the PluresDB adapter is wired.
		// Each plugin's onActivate(ctx) receives a pluginId-scoped PluginContext so
		// ctx.data.collection(name) persists under pluresdb:plugin:{pluginId}/...
		// (createPluginContext bridges the adapter; goto is injected for navigation).
		// Fire-and-forget: activation is async but must not block first paint;
		// per-plugin failures are isolated and logged inside activateAll.
		//
		// Enable/startup gate: a plugin activates on boot only if it is enabled AND
		// its startup policy is on. Both come from hydrated, persisted admin facts
		// (admin.plugins.enabled / admin.plugins.startup), so an operator's disable
		// or startup-off choice survives a restart. Absent id => enabled + startup-on
		// (opt-out model). Disabled/startup-off plugins stay registered and can be
		// activated on demand from the Plugins page without a reboot.
		void activateAll(
			(pluginId) => createPluginContext(pluginId, { goto }),
			(pluginId) => {
				const enabled = query('admin.plugins.enabled') as Record<string, boolean> | undefined;
				const startup = query('admin.plugins.startup') as Record<string, boolean> | undefined;
				return isPluginEnabled(enabled, pluginId) && shouldActivateOnStartup(startup, pluginId);
			},
		).then(() => {
			// Re-derive nav.visible now that agent-type/registry plugins are active,
			// so registry-contributed items (e.g. Agens → /agent) appear in the sidebar.
			seedNavItems();
		});

		// Initialize the design mode schema registry from all loaded praxis modules
		const schemas = buildSchemaRegistry(shellModule, agensModule, designModule, operationsModule, adminModule);
		emitFact('design.schema.registry', schemas);

		// Register modules for hot-reload
		registerForHotReload(shellModule);
		registerForHotReload(agensModule);
		registerForHotReload(designModule);
		registerForHotReload(operationsModule);
		registerForHotReload(adminModule);

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
	// eslint-disable-next-line plures/no-raw-stores
	let themeValue = $derived(
		(query<{ value: 'light' | 'dark' }>('theme.applied')?.value) ?? getTheme()
	);
	// eslint-disable-next-line plures/no-raw-stores
	let navItems = $derived(
		(query<{ items: { href: string; label: string; icon?: string; badge?: number }[] }>('nav.visible')?.items) ?? []
	);
	// eslint-disable-next-line plures/no-raw-stores
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
			// eslint-disable-next-line plures/no-manual-logging
			console.error('Failed to sync tray menu', error);
		});
	});

	// eslint-disable-next-line plures/no-raw-stores
	let sidebarCollapsed = $state(false);
	// eslint-disable-next-line plures/no-raw-stores
	let paletteOpen = $state(false);
	// eslint-disable-next-line plures/no-raw-stores
	let renderMode = $state<RenderMode>(detectRenderMode());

	// Built-in platform commands for the command palette
	// eslint-disable-next-line plures/no-raw-stores
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

	// eslint-disable-next-line plures/no-raw-stores
	let statusItems = $derived([
		{ label: 'Theme', value: themeValue },
		{ label: 'Render', value: renderMode === 'gui' ? 'GUI' : renderMode === 'tui-css' ? '⌨️ TUI' : '📟 Native' },
		{ label: 'Radix', value: 'v0.2.0' },
		...(designModeActive ? [{ label: 'Design', value: '🎨 Active' }] : []),
	]);
</script>

<Box class={`app ${renderModeClass(renderMode)}`} data-theme={themeValue}>
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
		<Breadcrumbs />
		{@render children()}
	</PluginContentArea>

	<CommandPalette
		bind:open={paletteOpen}
		{commands}
		onclose={() => (paletteOpen = false)}
	/>

	{#if renderMode === 'tui-css'}
		{@html `<style>${tuiCssOverrides}</style>`}
	{/if}
</Box>

<style>
	:global(:root), :global([data-theme="light"]) {
		--color-bg: #f8f9fa;
		--color-surface: #ffffff;
		--surface-1: #ffffff;
		--surface-2: #f3f4f6;
		--surface-3: #e5e7eb;
		--color-border: #e2e5e9;
		--color-text: #1a1d21;
		--text-primary: #1a1d21;
		--text-secondary: #6b7280;
		--color-text-muted: #6b7280;
		--color-accent: #4f46e5;
		--color-accent-bg: rgba(79, 70, 229, 0.1);
		--color-hover: rgba(0, 0, 0, 0.04);
		--color-danger: #dc2626;
	}

	:global([data-theme="dark"]) {
		--color-bg: #0f1117;
		--color-surface: #1a1d27;
		--surface-1: #1a1d27;
		--surface-2: #242836;
		--surface-3: #2d3140;
		--color-border: #2d3140;
		--color-text: #e2e5eb;
		--text-primary: #e2e5eb;
		--text-secondary: #8b92a5;
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
	:global(.app) { display: flex; height: 100vh; overflow: hidden; }
</style>
