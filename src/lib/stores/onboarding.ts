// Onboarding completion tracking — persisted to PluresDB
import { getSharedGraph } from './plures-db-adapter.js';

type Subscriber<T> = (value: T) => void;

const ONBOARDING_KEY = 'radix-onboarding';

function createOnboardingStore() {
	const graph = getSharedGraph();
	const stored = graph.get(ONBOARDING_KEY);
	let completed = new Set<string>(Array.isArray(stored) ? stored : []);
	const subscribers = new Set<Subscriber<Set<string>>>();

	function notify() {
		for (const sub of subscribers) sub(completed);
	}

	function persist() {
		graph.put(ONBOARDING_KEY, [...completed]);
	}

	const completedStore = {
		subscribe(run: Subscriber<Set<string>>) {
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
