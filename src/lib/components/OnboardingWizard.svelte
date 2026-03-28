<script lang="ts">
	import { Button } from '@plures/design-dojo';
	import type { OnboardingStep } from '$lib/types/plugin.js';
	import { onboarding } from '$lib/stores/onboarding.js';

	interface Props {
		steps: OnboardingStep[];
	}

	let { steps }: Props = $props();

	let currentIndex = $derived(
		steps.findIndex(s => !onboarding.isComplete(s.title))
	);

	let progress = $derived(
		steps.length > 0
			? Math.round((steps.filter(s => onboarding.isComplete(s.title)).length / steps.length) * 100)
			: 100
	);

	async function checkAndAdvance(step: OnboardingStep) {
		const complete = await step.isComplete();
		if (complete) {
			onboarding.markComplete(step.title);
		}
	}
</script>

<div class="onboarding">
	<div class="onboarding-header">
		<h2>Welcome to Radix</h2>
		<p>Complete these steps to get started</p>
		<div class="progress-bar">
			<div class="progress-fill" style="width: {progress}%"></div>
		</div>
		<span class="progress-label">{progress}% complete</span>
	</div>

	<div class="steps">
		{#each steps as step, i}
			{@const done = onboarding.isComplete(step.title)}
			{@const isCurrent = i === currentIndex}
			<div class="step" class:done class:current={isCurrent}>
				<div class="step-icon">{done ? '✅' : step.icon}</div>
				<div class="step-content">
					<h3>{step.title}</h3>
					<p>{step.description}</p>
					{#if isCurrent && !done}
						<div class="step-actions">
							<a href={step.href} class="btn primary">{step.actionLabel}</a>
							<Button variant="secondary" onclick={() => checkAndAdvance(step)}>
								I've done this
							</Button>
						</div>
					{/if}
				</div>
			</div>
		{/each}
	</div>
</div>

<style>
	.onboarding {
		max-width: 640px;
		margin: 0 auto;
	}

	.onboarding-header {
		text-align: center;
		margin-bottom: 32px;
	}

	.onboarding-header h2 {
		margin: 0 0 4px;
		color: var(--color-text);
	}

	.onboarding-header p {
		color: var(--color-text-muted);
		margin: 0 0 16px;
	}

	.progress-bar {
		height: 6px;
		background: var(--color-border);
		border-radius: 3px;
		overflow: hidden;
	}

	.progress-fill {
		height: 100%;
		background: var(--color-accent);
		transition: width 0.3s ease;
		border-radius: 3px;
	}

	.progress-label {
		font-size: 0.8rem;
		color: var(--color-text-muted);
	}

	.steps {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.step {
		display: flex;
		gap: 16px;
		padding: 16px;
		border-radius: 8px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		opacity: 0.6;
		transition: all 0.2s ease;
	}

	.step.current {
		opacity: 1;
		border-color: var(--color-accent);
	}

	.step.done {
		opacity: 0.8;
	}

	.step-icon {
		font-size: 1.5rem;
		flex-shrink: 0;
	}

	.step-content h3 {
		margin: 0 0 4px;
		font-size: 1rem;
		color: var(--color-text);
	}

	.step-content p {
		margin: 0;
		font-size: 0.85rem;
		color: var(--color-text-muted);
	}

	.step-actions {
		display: flex;
		gap: 8px;
		margin-top: 12px;
	}

	.btn {
		padding: 6px 16px;
		border-radius: 6px;
		font-size: 0.85rem;
		text-decoration: none;
		cursor: pointer;
		border: none;
		font-weight: 500;
	}

	.btn.primary {
		background: var(--color-accent);
		color: white;
	}

	.btn.secondary {
		background: var(--color-hover);
		color: var(--color-text);
	}
</style>
