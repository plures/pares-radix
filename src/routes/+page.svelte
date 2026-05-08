<script lang="ts">
	import { FirstRunWizard, DashboardGrid, Heading } from '@plures/design-dojo';
	import { getAllOnboardingSteps, getAllDashboardWidgets } from '$lib/platform/plugin-loader.js';
	import { onboarding } from '$lib/stores/onboarding.js';

	// eslint-disable-next-line plures/no-raw-stores -- $derived reads from PluresDB-backed onboarding store, not raw state
	let steps = $derived(getAllOnboardingSteps());
	// eslint-disable-next-line plures/no-raw-stores
	let widgets = $derived(getAllDashboardWidgets());
	const { completed } = onboarding;
	// eslint-disable-next-line plures/no-raw-stores
	let completedSteps = $derived($completed);
	// eslint-disable-next-line plures/no-raw-stores
	let onboardingComplete = $derived(
		steps.length === 0 || steps.every(s => completedSteps.has(s.title))
	);
</script>

<svelte:head>
	<title>Radix — Dashboard</title>
</svelte:head>

{#if onboardingComplete}
	<Heading level={1}>Dashboard</Heading>
	<DashboardGrid {widgets} />
{:else}
	<FirstRunWizard {steps} isComplete={onboarding.isComplete} markComplete={onboarding.markComplete} />
{/if}
