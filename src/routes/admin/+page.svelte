<script lang="ts">
	/**
	 * Admin Console — the operator window into the running Radix platform.
	 *
	 * PURE VIEW: this route contains no business logic. On mount it gathers the
	 * REAL runtime state (active plugins from the loader; gates + constraints from
	 * every loaded praxis module) and emits the admin.*.requested events. The
	 * admin praxis rules (src/lib/praxis/admin.ts, the twin of admin-console.px)
	 * compute admin.system.readiness / admin.plugins.health / admin.action.verdict,
	 * which this component then projects. All writes go through emitFact — never
	 * db.put (C-PLURES-003).
	 */
	import { onMount } from 'svelte';
	import { Box, Heading, Text, Badge, Toggle, Button, Card } from '@plures/design-dojo';
	import { PluginModule, BeaconBadge } from '@plures/design-dojo';
	import type { BeaconStatus } from '@plures/design-dojo';
	import { query, emitFact } from '$lib/stores/praxis-svelte.svelte.js';
	import { getPluginIds, getPlugin, isPluginActive } from '$lib/platform/plugin-loader.js';
	import { getSharedGraph } from '$lib/stores/plures-db-adapter.js';
	import { shellModule } from '$lib/praxis/shell.js';
	import { agensModule } from '$lib/praxis/agens.js';
	import { designModule } from '$lib/praxis/design.js';
	import { operationsModule } from '$lib/praxis/operations.js';
	import {
		adminModule,
		wireAdminScene,
		type PluginHealth,
		type SystemReadiness,
		type FeatureFlag,
	} from '$lib/praxis/admin.js';
	import type { PraxisModule, PraxisSystemState } from '$lib/types/praxis.js';

	// All modules whose gates + constraints the console reports on.
	const modules: PraxisModule[] = [
		shellModule,
		agensModule,
		designModule,
		operationsModule,
		adminModule,
	];

	// Reactive projections of the facts the admin rules emit. query() IS the
	// sanctioned reactive read; $derived just memoises it (same idiom as +layout).
	// eslint-disable-next-line plures/no-raw-stores
	let readiness = $derived(query<SystemReadiness>('admin.system.readiness'));
	// eslint-disable-next-line plures/no-raw-stores
	let roster = $derived(query<PluginHealth[]>('admin.plugins.health') ?? []);
	// eslint-disable-next-line plures/no-raw-stores
	let flags = $derived(query<FeatureFlag[]>('admin.feature.flags') ?? []);
	// eslint-disable-next-line plures/no-raw-stores
	let violations = $derived(
		query<Array<{ id: string; message: string }>>('admin.constraint.violations') ?? [],
	);
	// eslint-disable-next-line plures/no-raw-stores
	let auditLog = $derived(
		query<Array<{ action: string; target: string; verdict: string; reason: string; at: string }>>(
			'admin.audit.log',
		) ?? [],
	);
	// eslint-disable-next-line plures/no-raw-stores
	let showAudit = $derived(flags.find((f) => f.key === 'admin.showAuditLog')?.enabled ?? true);

	/** Build a live PraxisSystemState snapshot from the current facts + module gates. */
	function liveState(): PraxisSystemState {
		const facts = new Map<string, unknown>();
		// Seed the state with fact ids the constraints/gates read. The shared graph
		// is the source of truth; the reactive query() mirrors it.
		for (const mod of modules) {
			for (const f of mod.facts) {
				const v = query(f.id);
				if (v !== undefined) facts.set(f.id, v);
			}
		}
		return { facts };
	}

	/** Recompute the live scene: plugin roster, gate/violation readiness, flags. */
	function refresh(): void {
		wireAdminScene(emitFact, (id) => query(id));

		// 1. Real plugin roster from the loader.
		const plugins = getPluginIds().map((id) => {
			const p = getPlugin(id);
			const active = isPluginActive(id);
			const surface = (p?.routes?.length ?? 0) + (p?.navItems?.length ?? 0);
			return {
				pluginId: id,
				name: p?.name ?? id,
				version: p?.version ?? '—',
				active,
				surface,
			};
		});
		emitFact('admin.plugin.health.requested', { plugins });

		// 2. Real gate + constraint state across every loaded module.
		const state = liveState();
		let openGates = 0;
		let totalGates = 0;
		for (const mod of modules) {
			for (const g of mod.gates) {
				totalGates += 1;
				try {
					if (g.check(state)) openGates += 1;
				} catch {
					/* a throwing gate counts as closed */
				}
			}
		}
		const failing: Array<{ id: string; message: string }> = [];
		for (const mod of modules) {
			for (const c of mod.constraints) {
				try {
					if (!c.check(state)) failing.push({ id: c.id, message: c.message });
				} catch {
					failing.push({ id: c.id, message: c.message });
				}
			}
		}
		failing.sort((a, b) => a.id.localeCompare(b.id));
		emitFact('admin.constraint.violations', failing);
		emitFact('admin.readiness.requested', {
			openGates,
			totalGates,
			errorViolations: failing.length,
		});
	}

	function toggleFlag(flag: FeatureFlag): void {
		const next = flags.map((f) => (f.key === flag.key ? { ...f, enabled: !f.enabled } : f));
		emitFact('admin.feature.flags', next);
		// Route the toggle through the guard so it is audited (always allowed).
		emitFact('admin.action.requested', {
			action: 'toggle-flag',
			target: flag.key,
			activeDependents: [],
		});
	}

	/** Plugin health rollup → beacon status for the physical module lamp. */
	function pluginBeacon(status: string): BeaconStatus {
		if (status === 'healthy') return 'healthy';
		if (status === 'degraded') return 'warning';
		if (status === 'failed') return 'critical';
		return 'idle';
	}

	onMount(() => {
		// Ensure the shared graph is wired before we read facts.
		getSharedGraph();
		refresh();
	});
</script>

<Box class="page admin">
	<Box class="page-header">
		<Heading level={1} class="page-title">🛠️ Admin Console</Heading>
		<Button onclick={refresh}>Refresh</Button>
	</Box>

	<!-- System readiness ------------------------------------------------------ -->
	<Card class="admin-card">
		<Heading level={2}>System readiness</Heading>
		{#if readiness}
			<Box class="readiness-row">
				<BeaconBadge status={readiness.operable ? 'healthy' : 'critical'} size={28} />
				<Badge variant={readiness.operable ? 'success' : 'danger'}>
					{readiness.operable ? 'OPERABLE' : 'NOT OPERABLE'}
				</Badge>
				<Text as="span" class="muted">
					Gates {readiness.openGates}/{readiness.totalGates} open · {readiness.violations}
					error violation{readiness.violations === 1 ? '' : 's'}
				</Text>
			</Box>
		{:else}
			<Text as="p" class="muted">Computing…</Text>
		{/if}
	</Card>

	<!-- Plugin health --------------------------------------------------------- -->
	<Card class="admin-card">
		<Heading level={2}>Plugins ({roster.length})</Heading>
		{#if roster.length === 0}
			<Text as="p" class="muted">No plugins registered.</Text>
		{:else}
			<Box class="plugin-bay">
				{#each roster as p (p.pluginId)}
					<PluginModule
						name={p.name}
						version={p.version}
						status={pluginBeacon(p.status)}
						active={p.status !== 'inactive'}
						surfaces={p.surface}
					/>
				{/each}
			</Box>
		{/if}
	</Card>

	<!-- Constraint violations ------------------------------------------------- -->
	<Card class="admin-card">
		<Heading level={2}>Constraint health</Heading>
		{#if violations.length === 0}
			<Box class="ok-row">
				<Badge variant="success">ALL HOLD</Badge>
				<Text as="span" class="muted">No constraint violations across loaded modules.</Text>
			</Box>
		{:else}
			<Box class="violation-list">
				{#each violations as v (v.id)}
					<Box class="violation-row">
						<Badge variant="danger">{v.id}</Badge>
						<Text as="span">{v.message}</Text>
					</Box>
				{/each}
			</Box>
		{/if}
	</Card>

	<!-- Feature flags --------------------------------------------------------- -->
	<Card class="admin-card">
		<Heading level={2}>Feature flags</Heading>
		{#each flags as flag (flag.key)}
			<Box class="flag-row">
				<Toggle checked={flag.enabled} onchange={() => toggleFlag(flag)} />
				<Box class="flag-text">
					<Text as="span" class="flag-label">{flag.label}</Text>
					<Text as="span" class="muted">{flag.description}</Text>
				</Box>
			</Box>
		{/each}
	</Card>

	<!-- Audit log ------------------------------------------------------------- -->
	{#if showAudit}
		<Card class="admin-card">
			<Heading level={2}>Audit log ({auditLog.length})</Heading>
			{#if auditLog.length === 0}
				<Text as="p" class="muted">No admin actions recorded yet.</Text>
			{:else}
				<Box class="audit-list">
					{#each auditLog.slice(-20).reverse() as entry, i (entry.at + i)}
						<Box class="audit-row">
							<Badge variant={entry.verdict === 'allowed' ? 'success' : 'danger'}>
								{entry.verdict}
							</Badge>
							<Text as="span" class="audit-action">{entry.action} → {entry.target}</Text>
							<Text as="span" class="muted">{entry.reason}</Text>
						</Box>
					{/each}
				</Box>
			{/if}
		</Card>
	{/if}
</Box>

<style>
	:global(.readiness-row) {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}
	:global(.plugin-bay) {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem 1.25rem;
		align-items: flex-end;
		padding-top: 0.5rem;
	}
</style>

