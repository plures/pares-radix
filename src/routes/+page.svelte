<script lang="ts">
	import { FirstRunWizard, DashboardGrid } from '@plures/design-dojo';
	import { getAllOnboardingSteps, getAllDashboardWidgets } from '$lib/platform/plugin-loader.js';
	import { onboarding } from '$lib/stores/onboarding.js';

	let steps = $derived(getAllOnboardingSteps());
	let widgets = $derived(getAllDashboardWidgets());
	let onboardingComplete = $derived(
		steps.length === 0 || steps.every(s => onboarding.isComplete(s.title))
	);
</script>

<svelte:head>
	<title>Radix — Dashboard</title>
</svelte:head>

{#if onboardingComplete}
	<h1>Dashboard</h1>
	<DashboardGrid {widgets} />
{:else}
	<FirstRunWizard {steps} isComplete={onboarding.isComplete} markComplete={onboarding.markComplete} />
{/if}
