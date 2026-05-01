<script lang="ts">
	/**
	 * Omniscient Index Widget — shows filesystem index stats.
	 */

	let stats = $state({
		totalFiles: 0,
		enriched: 0,
		pending: 0,
		lastScan: null as string | null,
		securityAlerts: 0,
	});

	// TODO: wire to omniscient plugin API via Tauri invoke
</script>

<div class="omniscient-widget">
	{#if stats.totalFiles === 0}
		<p class="empty">No files indexed yet.</p>
		<p class="hint">Run <code>/index ~/projects</code> to start.</p>
	{:else}
		<div class="stat-grid">
			<div class="stat">
				<span class="value">{stats.totalFiles.toLocaleString()}</span>
				<span class="label">Files</span>
			</div>
			<div class="stat">
				<span class="value">{stats.enriched.toLocaleString()}</span>
				<span class="label">Enriched</span>
			</div>
			<div class="stat">
				<span class="value">{stats.pending.toLocaleString()}</span>
				<span class="label">Pending</span>
			</div>
			{#if stats.securityAlerts > 0}
				<div class="stat alert">
					<span class="value">{stats.securityAlerts}</span>
					<span class="label">⚠️ Alerts</span>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.omniscient-widget { padding: 0.5rem 0; }
	.empty { color: var(--color-text-muted); font-size: 0.9rem; margin: 0; }
	.hint { color: var(--color-text-muted); font-size: 0.8rem; margin: 4px 0 0; }
	.hint code {
		background: var(--color-hover); padding: 2px 5px; border-radius: 3px;
		font-size: 0.8rem;
	}
	.stat-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
	.stat { text-align: center; }
	.stat .value { display: block; font-size: 1.5rem; font-weight: 600; color: var(--color-text); }
	.stat .label { font-size: 0.75rem; color: var(--color-text-muted); }
	.stat.alert .value { color: var(--color-danger); }
</style>
