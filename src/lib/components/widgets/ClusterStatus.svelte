<script lang="ts">
	/**
	 * Cluster Status Widget — shows rector node health at a glance.
	 */

	let nodes = $state([
		{ name: 'praxisbot', status: 'online', cpu: 12, memory: 45, role: 'gpu-worker' },
		{ name: 'surface', status: 'online', cpu: 8, memory: 32, role: 'desktop' },
		{ name: 'devbox', status: 'offline', cpu: 0, memory: 0, role: 'dev' },
	]);

	// TODO: wire to rector API via Tauri invoke
</script>

<div class="cluster-widget">
	<div class="node-list">
		{#each nodes as node}
			<div class="node" class:offline={node.status === 'offline'}>
				<span class="indicator" class:online={node.status === 'online'}></span>
				<span class="name">{node.name}</span>
				<span class="role">{node.role}</span>
				{#if node.status === 'online'}
					<span class="stats">{node.cpu}% · {node.memory}%</span>
				{/if}
			</div>
		{/each}
	</div>
</div>

<style>
	.cluster-widget { padding: 0.5rem 0; }
	.node-list { display: flex; flex-direction: column; gap: 8px; }
	.node {
		display: flex; align-items: center; gap: 8px;
		padding: 8px 12px; border-radius: 6px;
		background: var(--color-surface); border: 1px solid var(--color-border);
	}
	.node.offline { opacity: 0.5; }
	.indicator {
		width: 8px; height: 8px; border-radius: 50%;
		background: var(--color-danger);
	}
	.indicator.online { background: #22c55e; }
	.name { font-weight: 500; font-size: 0.9rem; }
	.role { color: var(--color-text-muted); font-size: 0.8rem; }
	.stats { margin-left: auto; font-size: 0.8rem; color: var(--color-text-muted); }
</style>
