// Theme store — dark/light mode, persisted to PluresDB
import { browser } from '$app/environment';
import { getSharedGraph } from './plures-db-adapter.js';

type Subscriber<T> = (value: T) => void;

const THEME_KEY = 'radix-theme';

function createPluresState<T>(key: string, fallback: T) {
	let current = fallback;
	let initialized = false;
	const subscribers = new Set<Subscriber<T>>();

	function init() {
		if (initialized || typeof window === 'undefined') return;
		initialized = true;
		try {
			const graph = getSharedGraph();
			const stored = graph.get(key) as T | undefined;
			if (stored !== undefined) current = stored;
		} catch { /* SSR */ }
	}

	function notify() {
		for (const sub of subscribers) sub(current);
	}

	return {
		get value() {
			return current;
		},
		set(value: T) {
			current = value;
			if (typeof window !== 'undefined') {
				try { getSharedGraph().put(key, current); } catch { /* SSR */ }
			}
			notify();
		},
		subscribe(run: Subscriber<T>) {
			init();
			run(current);
			subscribers.add(run);
			return () => subscribers.delete(run);
		},
	};
}

function createThemeStore() {
	const state = createPluresState<'light' | 'dark'>(THEME_KEY, 'dark');

	state.subscribe((value) => {
		if (browser) {
			document.documentElement.setAttribute('data-theme', value);
		}
	});

	return {
		subscribe: state.subscribe,
		get value() {
			return state.value;
		},
		set value(v: 'light' | 'dark') {
			state.set(v);
		},
		toggle() {
			state.set(state.value === 'dark' ? 'light' : 'dark');
		},
	};
}

export const theme = createThemeStore();
