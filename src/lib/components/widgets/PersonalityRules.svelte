<script lang="ts">
	/**
	 * Personality Rules Widget — shows active personality rules with confidence.
	 */

	let rules = $state([
		{ name: 'no-filler', confidence: 0.95, category: 'communication' },
		{ name: 'verify-before-claiming', confidence: 0.95, category: 'engineering' },
		{ name: 'push-without-asking', confidence: 0.98, category: 'workflow' },
		{ name: 'no-bandaids', confidence: 0.95, category: 'engineering' },
		{ name: 'resourceful-before-asking', confidence: 0.90, category: 'workflow' },
	]);

	// TODO: wire to PluresDB personality rules query
</script>

<div class="personality-widget">
	{#each rules as rule}
		<div class="rule">
			<span class="confidence" style="opacity: {rule.confidence}">{(rule.confidence * 100).toFixed(0)}%</span>
			<span class="name">{rule.name}</span>
			<span class="category">{rule.category}</span>
		</div>
	{/each}
	<a href="/settings" class="manage-link">Manage rules →</a>
</div>

<style>
	.personality-widget { padding: 0.5rem 0; }
	.rule {
		display: flex; align-items: center; gap: 8px;
		padding: 4px 0; font-size: 0.85rem;
	}
	.confidence {
		font-family: monospace; font-size: 0.75rem;
		color: var(--color-accent); min-width: 32px;
	}
	.name { font-weight: 500; }
	.category { margin-left: auto; font-size: 0.75rem; color: var(--color-text-muted); }
	.manage-link {
		display: block; margin-top: 8px; font-size: 0.8rem;
		color: var(--color-accent); text-decoration: none;
	}
</style>
