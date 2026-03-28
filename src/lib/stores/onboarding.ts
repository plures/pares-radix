// Onboarding completion tracking — persisted to localStorage
import { browser } from '$app/environment';

function createOnboardingStore() {
	const stored = browser ? localStorage.getItem('radix-onboarding') : null;
	let completed = $state<Set<string>>(new Set(stored ? JSON.parse(stored) : []));

	$effect(() => {
		if (browser) {
			localStorage.setItem('radix-onboarding', JSON.stringify([...completed]));
		}
	});

	return {
		get completed() { return completed; },
		isComplete(stepTitle: string) { return completed.has(stepTitle); },
		markComplete(stepTitle: string) {
			completed = new Set([...completed, stepTitle]);
		},
		get allDone() { return false; }, // overridden by consumer with step count
		reset() { completed = new Set(); }
	};
}

export const onboarding = createOnboardingStore();
