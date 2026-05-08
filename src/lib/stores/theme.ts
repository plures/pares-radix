// Theme store — dark/light mode, persisted to PluresDB
import { browser } from '$app/environment';
import { getSharedGraph } from './plures-db-adapter.js';

type Subscriber<T> = (value: T) => void;

const THEME_KEY = 'radix-theme';

function createPluresState<T>(key: string, fallback: T) {
	const graph = getSharedGraph();
	let current = (graph.get(key) as T) ?? fallback;
	const subscribers = new Set<Subscriber<T>>();

	function notify() {
		for (const sub of subscribers) sub(current);
	}

	return {
		get value() {
			return current;
		},
		set(value: T) {
			current = value;
			graph.put(key, current);
			notify();
		},
		subscribe(run: Subscriber<T>) {
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
