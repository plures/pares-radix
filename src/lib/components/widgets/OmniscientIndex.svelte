<script lang="ts">
	/**
	 * Omniscient Index Widget — shows filesystem index stats.
	 */
	import { Box, CodeBlock, Text } from '@plures/design-dojo';

	// eslint-disable-next-line plures/no-raw-stores
	let stats = $state({
		totalFiles: 0,
		enriched: 0,
		pending: 0,
		lastScan: null as string | null,
		securityAlerts: 0,
	});

	// TODO: wire to omniscient plugin API via Tauri invoke
</script>

<Box class="omniscient-widget">
	{#if stats.totalFiles === 0}
		<Text as="p" class="empty">No files indexed yet.</Text>
		<Box class="hint">
			<Text as="p">Run</Text>
			<CodeBlock>/index ~/projects</CodeBlock>
			<Text as="p">to start.</Text>
		</Box>
	{:else}
		<Box class="stat-grid">
			<Box class="stat">
				<Text as="span" class="value">{stats.totalFiles.toLocaleString()}</Text>
				<Text as="span" class="label">Files</Text>
			</Box>
			<Box class="stat">
				<Text as="span" class="value">{stats.enriched.toLocaleString()}</Text>
				<Text as="span" class="label">Enriched</Text>
			</Box>
			<Box class="stat">
				<Text as="span" class="value">{stats.pending.toLocaleString()}</Text>
				<Text as="span" class="label">Pending</Text>
			</Box>
			{#if stats.securityAlerts > 0}
				<Box class="stat alert">
					<Text as="span" class="value">{stats.securityAlerts}</Text>
					<Text as="span" class="label">⚠️ Alerts</Text>
				</Box>
			{/if}
		</Box>
	{/if}
</Box>

<style>
	:global(.omniscient-widget) { padding: 0.5rem 0; }
	:global(.empty) { color: var(--color-text-muted); font-size: 0.9rem; margin: 0; }
	:global(.hint) { display: flex; align-items: center; gap: 6px; color: var(--color-text-muted); font-size: 0.8rem; margin: 4px 0 0; }
	:global(.hint pre) { margin: 0; }
	:global(.stat-grid) { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
	:global(.stat) { text-align: center; }
	:global(.stat .value) { display: block; font-size: 1.5rem; font-weight: 600; color: var(--color-text); }
	:global(.stat .label) { font-size: 0.75rem; color: var(--color-text-muted); }
	:global(.stat.alert .value) { color: var(--color-danger); }
</style>
