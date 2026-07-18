/**
 * Plugin loader — activation gate tests (C-TEST-002: channel-independent).
 *
 * These are BEHAVIORAL tests: they register real RadixPlugin objects (each with
 * a real onActivate/onDeactivate that records whether it ran) and drive the
 * actual loader — no mocks, no transport. They pin the exact bug class the
 * feature exists to prevent: a DISABLED (or startup-off) plugin must NOT be
 * activated on boot, and enable/disable must actually take effect at runtime.
 *
 * A prior version of activateAll activated every registered plugin
 * unconditionally; the eligibility predicate added here is what these tests
 * guard. If someone removes the gate, `disabled.activated` flips to true and
 * this suite fails loudly.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import {
	registerPlugin,
	activateAll,
	activatePlugin,
	deactivatePlugin,
	isPluginActive,
	__resetRegistryForTest,
} from './plugin-loader.js';
import { isPluginEnabled, shouldActivateOnStartup } from '../praxis/admin.js';
import type { RadixPlugin, PluginContext } from '../types/plugin.js';

/** A real plugin that records activation/deactivation — no stub, no mock. */
function makeSpyPlugin(id: string) {
	const calls = { activated: false, deactivated: false };
	const plugin: RadixPlugin = {
		id,
		name: id,
		version: '1.0.0',
		icon: '🧩',
		description: `${id} test plugin`,
		routes: [],
		navItems: [],
		settings: [],
		onActivate: async (_ctx: PluginContext) => {
			calls.activated = true;
		},
		onDeactivate: async () => {
			calls.deactivated = true;
		},
	};
	return { plugin, calls };
}

const ctx = {} as PluginContext;

describe('plugin-loader - activation eligibility gate', () => {
	beforeEach(() => {
		__resetRegistryForTest();
	});

	it('activateAll activates an enabled plugin and SKIPS a disabled one', async () => {
		const on = makeSpyPlugin('enabled-plugin');
		const off = makeSpyPlugin('disabled-plugin');
		registerPlugin(on.plugin);
		registerPlugin(off.plugin);

		const enabledMap = { 'disabled-plugin': false };
		const startupMap: Record<string, boolean> = {};

		await activateAll(
			() => ctx,
			(id) => isPluginEnabled(enabledMap, id) && shouldActivateOnStartup(startupMap, id),
		);

		expect(on.calls.activated).toBe(true);
		expect(isPluginActive('enabled-plugin')).toBe(true);
		// The gate must have prevented the disabled plugin from ever activating.
		expect(off.calls.activated).toBe(false);
		expect(isPluginActive('disabled-plugin')).toBe(false);
	});

	it('activateAll SKIPS an enabled-but-startup-off plugin on boot', async () => {
		const p = makeSpyPlugin('lazy-plugin');
		registerPlugin(p.plugin);

		const enabledMap: Record<string, boolean> = {}; // enabled
		const startupMap = { 'lazy-plugin': false }; // but not on startup

		await activateAll(
			() => ctx,
			(id) => isPluginEnabled(enabledMap, id) && shouldActivateOnStartup(startupMap, id),
		);

		expect(p.calls.activated).toBe(false);
		expect(isPluginActive('lazy-plugin')).toBe(false);
	});

	it('with no predicate, every registered plugin activates (default-on, back-compat)', async () => {
		const a = makeSpyPlugin('a');
		const b = makeSpyPlugin('b');
		registerPlugin(a.plugin);
		registerPlugin(b.plugin);

		await activateAll(() => ctx);

		expect(a.calls.activated).toBe(true);
		expect(b.calls.activated).toBe(true);
	});

	it('activatePlugin activates a startup-off plugin on demand (idempotent)', async () => {
		const p = makeSpyPlugin('on-demand');
		registerPlugin(p.plugin);
		// Not activated on boot (startup-off gate).
		await activateAll(
			() => ctx,
			() => false,
		);
		expect(isPluginActive('on-demand')).toBe(false);

		// Operator enables it → on-demand activation.
		const first = await activatePlugin('on-demand', () => ctx);
		expect(first).toBe(true);
		expect(p.calls.activated).toBe(true);
		expect(isPluginActive('on-demand')).toBe(true);

		// Idempotent: calling again is a no-op success.
		const second = await activatePlugin('on-demand', () => ctx);
		expect(second).toBe(true);
	});

	it('deactivatePlugin tears down a running plugin (idempotent)', async () => {
		const p = makeSpyPlugin('running');
		registerPlugin(p.plugin);
		await activateAll(() => ctx);
		expect(isPluginActive('running')).toBe(true);

		const ok = await deactivatePlugin('running');
		expect(ok).toBe(true);
		expect(p.calls.deactivated).toBe(true);
		expect(isPluginActive('running')).toBe(false);

		// Idempotent: deactivating an inactive plugin is a no-op success.
		expect(await deactivatePlugin('running')).toBe(true);
	});
});
