/**
 * Navigation Guard — prevents accidental navigation away from unsaved work.
 *
 * Usage in a +page.svelte:
 *   import { useNavGuard } from '$lib/platform/nav-guard.js';
 *   const guard = useNavGuard();
 *   guard.setDirty(true);  // mark as having unsaved changes
 *   guard.setDirty(false); // clear after save
 */

import { beforeNavigate } from '$app/navigation';
import { onMount } from 'svelte';

export interface NavGuard {
	setDirty(dirty: boolean): void;
	isDirty(): boolean;
}

/**
 * Create a navigation guard. Call in a component's <script> block.
 * When dirty=true, browser navigation and SvelteKit route changes
 * will prompt the user before leaving.
 */
export function useNavGuard(message = 'You have unsaved changes. Leave anyway?'): NavGuard {
	let dirty = false;

	// Guard SvelteKit client-side navigation
	beforeNavigate(({ cancel }) => {
		if (dirty && !confirm(message)) {
			cancel();
		}
	});

	// Guard browser-level navigation (reload, close tab, external link)
	onMount(() => {
		function handleBeforeUnload(e: BeforeUnloadEvent) {
			if (dirty) {
				e.preventDefault();
				// Modern browsers ignore custom messages but still show prompt
				e.returnValue = message;
			}
		}
		window.addEventListener('beforeunload', handleBeforeUnload);
		return () => window.removeEventListener('beforeunload', handleBeforeUnload);
	});

	return {
		setDirty(value: boolean) {
			dirty = value;
		},
		isDirty() {
			return dirty;
		},
	};
}
