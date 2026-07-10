<!-- @component
  RegionMap — a geographic region drawn as a territory plot that CONTAINS its
  datacenters. A soft landmass with a region tag and a subtle grid/coastline,
  hosting DatacenterBuilding children via a snippet. Expresses "datacenters are
  found in regions of a country."

  Usage:
    <RegionMap region="West US 2" code="westus2" tone="healthy">
      {#snippet children()}
        <DatacenterBuilding ... />
      {/snippet}
    </RegionMap>
-->
<script lang="ts">
	import type { Snippet } from "svelte";
	import type { RegionTone } from "../types-local.js";

	interface Props {
		region: string;
		/** Short cloud region code, e.g. westus2. */
		code?: string;
		tone?: RegionTone;
		/** Optional right-aligned metric, e.g. "3 DCs · 98% SLO". */
		metric?: string;
		children: Snippet;
	}

	let { region, code, tone = "neutral", metric, children }: Props = $props();

	const accentFor: Record<RegionTone, string> = {
		healthy: "var(--color-success)",
		warning: "var(--color-warning)",
		critical: "var(--color-danger)",
		neutral: "var(--color-accent)",
	};
	let accent = $derived(accentFor[tone]);
</script>

<section class="region" style={`--region-accent:${accent}`}>
	<header class="region__tag">
		<span class="region__pin" aria-hidden="true"></span>
		<span class="region__name">{region}</span>
		{#if code}<code class="region__code">{code}</code>{/if}
		{#if metric}<span class="region__metric">{metric}</span>{/if}
	</header>
	<div class="region__plot">
		{@render children()}
	</div>
</section>

<style>
	.region {
		border: 1px dashed color-mix(in srgb, var(--region-accent) 55%, var(--color-border));
		border-radius: 14px;
		padding: 0.5rem 0.75rem 0.9rem;
		background:
			radial-gradient(
				circle at 18% 12%,
				color-mix(in srgb, var(--region-accent) 10%, transparent),
				transparent 60%
			),
			repeating-linear-gradient(
				45deg,
				transparent,
				transparent 11px,
				color-mix(in srgb, var(--color-border) 30%, transparent) 11px,
				color-mix(in srgb, var(--color-border) 30%, transparent) 12px
			),
			var(--color-bg-content);
	}
	.region__tag {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.6rem;
	}
	.region__pin {
		width: 10px;
		height: 10px;
		border-radius: 50% 50% 50% 0;
		transform: rotate(-45deg);
		background: var(--region-accent);
		box-shadow: 0 0 8px color-mix(in srgb, var(--region-accent) 70%, transparent);
	}
	.region__name {
		font-weight: 700;
		font-size: 0.9rem;
		color: var(--color-text);
	}
	.region__code {
		font-size: 0.7rem;
		color: var(--color-text-muted);
		background: var(--color-bg-active);
		padding: 0.05rem 0.35rem;
		border-radius: 4px;
	}
	.region__metric {
		margin-left: auto;
		font-size: 0.72rem;
		color: var(--color-text-subtle);
	}
	.region__plot {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem;
		align-items: flex-end;
	}
</style>