<script lang="ts">
	import { Heading, Text, Box } from "@plures/design-dojo";
	import { RegionMap, ServerRack, DatacenterBuilding } from "@plures/design-dojo";
	import { query } from "$lib/stores/praxis-svelte.svelte.js";
	import type { ServiceRecord } from "$lib/praxis/operations.js";
	import {
		unitsFor,
		groupByRegion,
		regionTone,
		regionMetric,
		regionWindows,
		prettyRegion,
		type RegionGroup,
	} from "$lib/praxis/operations-view.js";

	// The fleet is real, constraint-checked praxis state seeded via emitFact.
	// eslint-disable-next-line plures/no-raw-stores -- query() is the sanctioned reactive read
	let fleet = $derived((query("ops.fleet.services") as ServiceRecord[] | undefined) ?? []);
	// eslint-disable-next-line plures/no-raw-stores
	let freeze = $derived(Boolean(query("ops.change.freeze")));

	let regions = $derived.by<RegionGroup[]>(() => groupByRegion(fleet));
</script>

<svelte:head>
	<title>Radix — Operations</title>
</svelte:head>

<Heading level={1}>Operations</Heading>
<Text as="p" class="ops-sub">
	The live fleet as it physically exists — services are racks of servers, replicas
	are sleds, and each region hosts a sovereign datacenter. Lights follow real
	SLO-derived health.
</Text>

{#if freeze}
	<Box class="ops-freeze">❄ Change freeze in effect — deploy/scale intents are being deferred.</Box>
{/if}

{#if regions.length === 0}
	<Box class="ops-empty"><Text>No fleet state seeded yet.</Text></Box>
{:else}
	<Box class="ops-regions">
		{#each regions as r (r.code)}
			<RegionMap
				region={prettyRegion(r.code)}
				code={r.code}
				tone={regionTone(r.services)}
				metric={regionMetric(r.services)}
			>
				{#snippet children()}
					<DatacenterBuilding
						name={`${r.code.toUpperCase()}-DC`}
						windows={regionWindows(r.services)}
						sovereign={r.services.every((s) => s.sovereign)}
						caption="sovereign facility"
					/>
					{#each r.services as s (s.id)}
						<ServerRack
							label={s.name}
							units={unitsFor(s)}
							caption={`${s.tier} · v${s.version} · ${Math.round(s.errorBudget * 100)}% budget`}
						/>
					{/each}
				{/snippet}
			</RegionMap>
		{/each}
	</Box>
{/if}

<style>
	:global(.ops-sub) {
		color: var(--color-text-muted);
		max-width: 46rem;
		margin: 0.25rem 0 1.25rem;
	}
	:global(.ops-regions) {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}
	:global(.ops-freeze) {
		background: color-mix(in srgb, var(--color-info) 15%, var(--color-bg-card));
		border: 1px solid var(--color-info);
		border-radius: 8px;
		padding: 0.6rem 0.9rem;
		margin-bottom: 1.1rem;
		color: var(--color-text);
	}
	:global(.ops-empty) {
		padding: 2rem;
		text-align: center;
		color: var(--color-text-subtle);
	}
</style>