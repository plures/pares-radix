/**
 * Tauri 2 platform bridge — events not commands
 *
 * This module provides a browser-safe wrapper around @tauri-apps/api.
 * All functions are no-ops in the browser (SvelteKit dev / SSR) so the
 * app works identically in both environments.
 *
 * Architecture: Tauri commands are thin wrappers that emit praxis events.
 * This file translates:
 *   - Frontend → Rust:   invoke() for commands that emit events
 *   - Rust → Frontend:   listen() handlers that call emitFact()
 *
 * Anti-patterns (DO NOT):
 *   ✗ No business logic here — all logic lives in praxis rules
 *   ✗ No state stored here — state is always a praxis fact
 */

import { browser } from '$app/environment';

// ─── Runtime detection ────────────────────────────────────────────────────────

/**
 * Returns true when running inside a Tauri desktop window.
 * Always false in the browser (SvelteKit dev / static build).
 */
export function isTauri(): boolean {
	return browser && '__TAURI_INTERNALS__' in window;
}

// ─── Lazy imports — only resolved inside Tauri ────────────────────────────────

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
type ListenFn = <T>(event: string, handler: (payload: { payload: T }) => void) => Promise<() => void>;

let _invoke: InvokeFn | null = null;
let _listen: ListenFn | null = null;

async function getInvoke(): Promise<InvokeFn | null> {
	if (!isTauri()) return null;
	if (!_invoke) {
		const mod = await import('@tauri-apps/api/core');
		_invoke = mod.invoke as InvokeFn;
	}
	return _invoke;
}

async function getListen(): Promise<ListenFn | null> {
	if (!isTauri()) return null;
	if (!_listen) {
		const mod = await import('@tauri-apps/api/event');
		_listen = mod.listen as ListenFn;
	}
	return _listen;
}

// ─── Praxis event payloads (mirrors src-tauri/src/lib.rs) ────────────────────

export interface WindowStatePayload {
	x: number;
	y: number;
	width: number;
	height: number;
	maximized: boolean;
}

export interface TrayMenuItem {
	id: string;
	label: string;
	path: string;
}

export interface AppBootedPayload {
	version: string;
}

// ─── Commands (thin wrappers — emit praxis events in Rust) ───────────────────

/**
 * Navigate to a path by asking the Rust backend to emit "user-navigated".
 * The frontend listens for that event and routes via `emitFact('user.navigated', …)`.
 */
export async function tauriNavigate(path: string): Promise<void> {
	const invoke = await getInvoke();
	if (!invoke) return;
	await invoke('navigate', { path });
}

/**
 * Ask the Rust backend to restore window geometry.
 * Returns null in browser (no-op) or if the window state is unavailable.
 */
export async function tauriGetWindowState(): Promise<WindowStatePayload | null> {
	const invoke = await getInvoke();
	if (!invoke) return null;
	try {
		return await invoke<WindowStatePayload>('get_window_state');
	} catch {
		return null;
	}
}

/**
 * Push the current nav.visible items to the Rust tray menu builder.
 * Called automatically whenever the `nav.visible` fact changes.
 */
export async function tauriSetTrayMenu(items: TrayMenuItem[]): Promise<void> {
	const invoke = await getInvoke();
	if (!invoke) return;
	await invoke('set_tray_menu', { items });
}

/**
 * Persist the current window geometry.
 * Called from the window-state-changed handler below.
 */
export async function tauriSaveWindowState(state: WindowStatePayload): Promise<void> {
	const invoke = await getInvoke();
	if (!invoke) return;
	await invoke('save_window_state', { state });
}

// ─── Event listeners (Rust → Frontend praxis facts) ──────────────────────────

/**
 * Callback type used by `listenTauriEvents`.
 * Returns unlisten functions — call them on component destroy to clean up.
 */
export interface TauriEventHandlers {
	/** Called when Rust emits "app-booted". Seed `app.ready` fact here. */
	onAppBooted?: (payload: AppBootedPayload) => void;
	/** Called when Rust emits "window-state-changed". Persist `app.window` fact. */
	onWindowStateChanged?: (payload: WindowStatePayload) => void;
	/** Called when Rust emits "user-navigated". Route via `emitFact('user.navigated', …)`. */
	onUserNavigated?: (payload: { path: string }) => void;
}

/**
 * Register all Tauri → praxis event listeners.
 *
 * Returns a cleanup function (call in onDestroy / $effect cleanup).
 * No-op when not running inside Tauri.
 *
 * @example
 * onMount(async () => {
 *   const unlisten = await listenTauriEvents({
 *     onWindowStateChanged: (s) => emitFact('app.window', s),
 *     onUserNavigated: ({ path }) => emitFact('user.navigated', { path }),
 *   });
 *   return unlisten;
 * });
 */
export async function listenTauriEvents(handlers: TauriEventHandlers): Promise<() => void> {
	const listen = await getListen();
	if (!listen) return () => {};

	const unlisteners: Array<() => void> = [];

	if (handlers.onAppBooted) {
		const fn = handlers.onAppBooted;
		unlisteners.push(await listen<AppBootedPayload>('app-booted', (e) => fn(e.payload)));
	}
	if (handlers.onWindowStateChanged) {
		const fn = handlers.onWindowStateChanged;
		unlisteners.push(
			await listen<WindowStatePayload>('window-state-changed', (e) => fn(e.payload)),
		);
	}
	if (handlers.onUserNavigated) {
		const fn = handlers.onUserNavigated;
		unlisteners.push(
			await listen<{ path: string }>('user-navigated', (e) => fn(e.payload)),
		);
	}

	return () => unlisteners.forEach((fn) => fn());
}
