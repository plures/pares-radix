/**
 * Plugin store — reactive state for installed plugins.
 *
 * Bridges the Tauri plugin commands with Svelte's reactivity system.
 * Falls back to the legacy plugin-loader IDs when running outside Tauri.
 */
import { getPluginIds, isPluginActive } from '$lib/platform/plugin-loader.js';
import { listPlugins, installPlugin, uninstallPlugin } from '$lib/plugins/plugin-api.js';
import type { PluginInfo } from '$lib/plugins/plugin-api.js';

function createPluginsStore() {
	let ids = $state<string[]>([]);
	let installed = $state<PluginInfo[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);

	return {
		get ids() {
			return ids;
		},
		get installed() {
			return installed;
		},
		get loading() {
			return loading;
		},
		get error() {
			return error;
		},

		/** Refresh from the Tauri backend. Falls back to legacy loader. */
		async refresh() {
			loading = true;
			error = null;
			try {
				installed = await listPlugins();
				ids = installed.map((p) => p.name);
			} catch {
				// Outside Tauri — use legacy plugin loader
				ids = getPluginIds();
				installed = [];
			} finally {
				loading = false;
			}
		},

		async install(path: string): Promise<string> {
			const name = await installPlugin(path);
			await this.refresh();
			return name;
		},

		async uninstall(name: string): Promise<void> {
			await uninstallPlugin(name);
			await this.refresh();
		},

		isActive(id: string) {
			return isPluginActive(id);
		}
	};
}

export const plugins = createPluginsStore();
