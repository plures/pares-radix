// Plugin state store
import { getPluginIds, isPluginActive } from '$lib/platform/plugin-loader.js';

function createPluginsStore() {
	let ids = $state<string[]>([]);

	return {
		get ids() { return ids; },
		refresh() {
			ids = getPluginIds();
		},
		isActive(id: string) {
			return isPluginActive(id);
		}
	};
}

export const plugins = createPluginsStore();
