<!-- @component
  ServerRack — renders a fleet of servers the way they physically exist:
  slim horizontal sleds stacked in a rack chassis, each with a status lamp
  and activity LEDs. Replaces the "flat card per service" metaphor.

  Each unit maps to one logical server/replica. The rack frame, rails and
  ventilation read as real hardware. Colour comes from design tokens only.

  Usage:
    <ServerRack label="orders-api" units={units} region="westus2" />
-->
<script lang="ts">
	import { Rect, Group, Text } from "@plures/design-dojo-npm/svg";
	import StatusBeacon from "./StatusBeacon.svelte";
	import type { BeaconStatus, RackUnit } from "../types-local.js";

	interface Props {
		label: string;
		units: RackUnit[];
		/** Sub-caption under the rack (region / tier / SLO). */
		caption?: string;
		width?: number;
		/** Height of a single sled. */
		unitHeight?: number;
		onunit?: (id: string) => void;
	}

	let {
		label,
		units,
		caption,
		width = 180,
		unitHeight = 20,
		onunit,
	}: Props = $props();

	const pad = 10;
	const railW = 7;
	const headerH = 26;
	const footerH = 16;
	let bodyH = $derived(units.length * (unitHeight + 4) + 6);
	let height = $derived(headerH + bodyH + footerH);
	let ledCount = 6;
</script>

<svg
	{width}
	{height}
	viewBox={`0 0 ${width} ${height}`}
	role="group"
	aria-label={`server rack ${label}, ${units.length} units`}
>
	<!-- chassis shell -->
	<Rect
		x={1}
		y={1}
		width={width - 2}
		height={height - 2}
		rx={7}
		fill="var(--color-bg-elevated)"
		stroke="var(--color-border)"
		strokeWidth={1.5}
	/>
	<!-- top bezel -->
	<Rect x={1} y={1} width={width - 2} height={headerH} rx={7} fill="var(--color-bg-sidebar)" />
	<Text x={pad} y={17} fill="var(--color-text)" fontSize={12} fontWeight={600}>{label}</Text>

	<!-- mounting rails -->
	<Rect x={pad - 4} y={headerH} width={railW} height={bodyH} fill="var(--color-bg-active)" />
	<Rect
		x={width - pad - railW + 4}
		y={headerH}
		width={railW}
		height={bodyH}
		fill="var(--color-bg-active)"
	/>

	<!-- server sleds -->
	{#each units as u, i (u.id)}
		<Group translateX={pad + railW - 1} translateY={headerH + 4 + i * (unitHeight + 4)}>
			<Rect
				width={width - 2 * (pad + railW) + 2}
				height={unitHeight}
				rx={3}
				fill="var(--color-bg-card)"
				stroke="var(--color-border-muted)"
				strokeWidth={1}
				role={onunit ? "button" : undefined}
				aria-label={onunit ? `unit ${u.label ?? u.id}` : undefined}
				onclick={onunit ? () => onunit(u.id) : undefined}
			/>
			<!-- status lamp -->
			<StatusBeacon status={u.status} x={12} y={unitHeight / 2} r={4} />
			<!-- unit label -->
			<Text x={24} y={unitHeight / 2 + 3.5} fill="var(--color-text-muted)" fontSize={9}>
				{u.label ?? u.id}
			</Text>
			<!-- activity LEDs (utilisation) -->
			{#each Array(ledCount) as _, k (k)}
				{@const w = width - 2 * (pad + railW) + 2}
				{@const on = (u.load ?? 0) * ledCount > k}
				<Rect
					x={w - 12 - k * 8}
					y={unitHeight / 2 - 4}
					width={5}
					height={8}
					rx={1}
					fill={on ? "var(--color-accent)" : "var(--color-bg-active)"}
					opacity={on ? 1 : 0.6}
				/>
			{/each}
		</Group>
	{/each}

	<!-- vented footer -->
	<Group translateX={pad} translateY={height - footerH + 4}>
		{#each Array(Math.floor((width - 2 * pad) / 8)) as _, v (v)}
			<Rect x={v * 8} y={0} width={4} height={7} rx={1.5} fill="var(--color-bg-active)" />
		{/each}
	</Group>
</svg>

{#if caption}
	<div class="rack-caption">{caption}</div>
{/if}

<style>
	.rack-caption {
		font-size: 0.72rem;
		color: var(--color-text-subtle);
		text-align: center;
		margin-top: 0.25rem;
		letter-spacing: 0.02em;
	}
</style>