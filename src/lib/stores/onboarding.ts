// Onboarding completion tracking — persisted to PluresDB
import { getSharedGraph } from './plures-db-adapter.js';

type Subscriber<T> = (value: T) => void;

const ONBOARDING_KEY = 'radix-onboarding';

function createOnboardingStore() {
	// Defer graph access to avoid SSR localStorage issues
	let completed = new Set<string>();
	let initialized = false;
	const subscribers = new Set<Subscriber<Set<string>>>();

	function init() {
		if (initialized) return;
		if (typeof window === 'undefined') return;
		initialized = true;
		try {
			const graph = getSharedGraph();
			const stored = graph.get(ONBOARDING_KEY);
			if (Array.isArray(stored)) completed = new Set(stored);
		} catch { /* SSR — no localStorage */ }
	}

	function notify() {
		for (const sub of subscribers) sub(completed);
	}

	function persist() {
		if (typeof window === 'undefined') return;
		try {
			const graph = getSharedGraph();
			graph.put(ONBOARDING_KEY, [...completed]);
		} catch { /* SSR */ }
	}

	const completedStore = {
		subscribe(run: Subscriber<Set<string>>) {
			init();
			run(completed);
			subscribers.add(run);
			return () => subscribers.delete(run);
		},
	};

	function setCompleted(next: Set<string>) {
		completed = next;
		persist();
		notify();
	}

	return {
		completed: completedStore,
		isComplete(stepTitle: string) { return completed.has(stepTitle); },
		markComplete(stepTitle: string) {
			setCompleted(new Set([...completed, stepTitle]));
		},
		get allDone() { return false; }, // overridden by consumer with step count
		reset() { setCompleted(new Set()); },
	};
}

export const onboarding = createOnboardingStore();
