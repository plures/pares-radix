<script lang="ts">
	import type { OnboardingStep } from '$lib/types/plugin.js';
	import { onboarding } from '$lib/stores/onboarding.js';

	interface Props {
		steps: OnboardingStep[];
	}

	let { steps }: Props = $props();

	let progress = $derived(
		steps.length > 0
			? Math.round((steps.filter(s => onboarding.isComplete(s.title)).length / steps.length) * 100)
			: 100
	);

	function isLocked(step: OnboardingStep): boolean {
		return (step.after ?? []).some(dep => !onboarding.isComplete(dep));
	}

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
		{#each steps as step}
			{@const done = onboarding.isComplete(step.title)}
			{@const locked = !done && isLocked(step)}
			<div class="step" class:done class:locked>
				<div class="step-icon">{done ? '✅' : locked ? '🔒' : step.icon}</div>
				<div class="step-content">
					<h3>{step.title}</h3>
					<p>{step.description}</p>
					{#if !done && !locked}
						<div class="step-actions">
							<a href={step.href} class="btn primary">{step.actionLabel}</a>
							<button class="btn secondary" onclick={() => checkAndAdvance(step)}>
								I've done this
							</button>
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
		transition: all 0.2s ease;
		/* active steps (not done, not locked) are fully visible at opacity 1 */
	}

	.step.done {
		opacity: 0.7;
	}

	.step.locked {
		opacity: 0.4;
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
