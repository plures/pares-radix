<script lang="ts">
	/**
	 * Cluster Status Widget — shows rector node health at a glance.
	 */
	import { Box, Text } from '@plures/design-dojo';

	// eslint-disable-next-line plures/no-raw-stores
	let nodes = $state([
		{ name: 'praxisbot', status: 'online', cpu: 12, memory: 45, role: 'gpu-worker' },
		{ name: 'surface', status: 'online', cpu: 8, memory: 32, role: 'desktop' },
		{ name: 'devbox', status: 'offline', cpu: 0, memory: 0, role: 'dev' },
	]);

	// TODO: wire to rector API via Tauri invoke
</script>

<Box class="cluster-widget">
	<Box class="node-list">
		{#each nodes as node}
			<Box class={`node ${node.status === 'offline' ? 'offline' : ''}`}>
				<Box as="span" class={`indicator ${node.status === 'online' ? 'online' : ''}`}></Box>
				<Text as="span" class="name">{node.name}</Text>
				<Text as="span" class="role">{node.role}</Text>
				{#if node.status === 'online'}
					<Text as="span" class="stats">{node.cpu}% · {node.memory}%</Text>
				{/if}
			</Box>
		{/each}
	</Box>
</Box>

<style>
	:global(.cluster-widget) { padding: 0.5rem 0; }
	:global(.node-list) { display: flex; flex-direction: column; gap: 8px; }
	:global(.node) {
		display: flex; align-items: center; gap: 8px;
		padding: 8px 12px; border-radius: 6px;
		background: var(--color-surface); border: 1px solid var(--color-border);
	}
	:global(.node.offline) { opacity: 0.5; }
	:global(.indicator) {
		width: 8px; height: 8px; border-radius: 50%;
		background: var(--color-danger);
	}
	:global(.indicator.online) { background: #22c55e; }
	:global(.name) { font-weight: 500; font-size: 0.9rem; }
	:global(.role) { color: var(--color-text-muted); font-size: 0.8rem; }
	:global(.stats) { margin-left: auto; font-size: 0.8rem; color: var(--color-text-muted); }
</style>
