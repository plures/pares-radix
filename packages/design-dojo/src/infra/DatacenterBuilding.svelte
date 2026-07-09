<!-- @component
  DatacenterBuilding — a datacenter drawn as a building: a facility silhouette
  with a lit window grid, where each window is a rack/room whose light encodes
  health. A red-lit window pulls the eye to a failing room, like looking at a
  real building at night. Replaces the flat "datacenter card".

  Usage:
    <DatacenterBuilding name="WESTUS2-DC3" windows={windows} sovereign />
-->
<script lang="ts">
	import { Rect, Group, Text, Path } from "@plures/design-dojo-npm/svg";
	import type { SvgFill } from "@plures/design-dojo-npm/svg";
	import type { BeaconStatus } from "./StatusBeacon.svelte";

	interface Props {
		name: string;
		/** One entry per room/rack; status lights the window. */
		windows: BeaconStatus[];
		/** Columns in the window grid. Default 6. */
		cols?: number;
		width?: number;
		/** Sovereign / regulated facility → gets a shield mark + accent trim. */
		sovereign?: boolean;
		caption?: string;
		onclick?: () => void;
	}

	let { name, windows, cols = 6, width = 150, sovereign = false, caption, onclick }: Props =
		$props();

	const winFill: Record<BeaconStatus, SvgFill> = {
		healthy: "var(--color-success)",
		warning: "var(--color-warning)",
		critical: "var(--color-danger)",
		idle: "var(--color-bg-active)",
		unknown: "var(--color-border)",
	};

	const roofH = 16;
	const pad = 12;
	const winSize = 12;
	const gap = 6;
	let rows = $derived(Math.max(1, Math.ceil(windows.length / cols)));
	let gridW = $derived(cols * winSize + (cols - 1) * gap);
	let bodyH = $derived(rows * winSize + (rows - 1) * gap + 2 * pad);
	let height = $derived(roofH + bodyH + 22);
	let gridX = $derived((width - gridW) / 2);
</script>

<svg
	{width}
	{height}
	viewBox={`0 0 ${width} ${height}`}
	role={onclick ? "button" : "group"}
	aria-label={`datacenter ${name}, ${windows.length} rooms`}
	{onclick}
	class={onclick ? "dc-interactive" : undefined}
>
	<!-- roof / parapet -->
	<Path
		d={`M6 ${roofH + 2} L${width * 0.2} 4 L${width * 0.8} 4 L${width - 6} ${roofH + 2} Z`}
		fill="var(--color-bg-sidebar)"
		stroke="var(--color-border)"
		strokeWidth={1.2}
	/>
	<!-- rooftop condenser units -->
	<Rect x={width * 0.32} y={6} width={12} height={6} rx={1} fill="var(--color-bg-active)" />
	<Rect x={width * 0.55} y={6} width={12} height={6} rx={1} fill="var(--color-bg-active)" />

	<!-- facility body -->
	<Rect
		x={6}
		y={roofH}
		width={width - 12}
		height={bodyH}
		rx={4}
		fill="var(--color-bg-elevated)"
		stroke={sovereign ? "var(--color-accent)" : "var(--color-border)"}
		strokeWidth={sovereign ? 2 : 1.4}
	/>

	<!-- window grid = rooms/racks -->
	<Group translateX={gridX} translateY={roofH + pad}>
		{#each windows as w, i (i)}
			{@const col = i % cols}
			{@const row = Math.floor(i / cols)}
			<Rect
				x={col * (winSize + gap)}
				y={row * (winSize + gap)}
				width={winSize}
				height={winSize}
				rx={1.5}
				fill={winFill[w]}
				opacity={w === "idle" ? 0.5 : 0.92}
				stroke="var(--color-bg)"
				strokeWidth={1}
			/>
		{/each}
	</Group>

	<!-- entrance -->
	<Rect
		x={width / 2 - 9}
		y={roofH + bodyH - 14}
		width={18}
		height={14}
		rx={1}
		fill="var(--color-bg-sidebar)"
	/>

	<!-- sovereign shield -->
	{#if sovereign}
		<Path
			d={`M${width - 20} ${roofH + 6} l7 2 l0 6 q0 5 -7 8 q-7 -3 -7 -8 l0 -6 Z`}
			fill="var(--color-accent)"
			opacity={0.9}
			aria-label="sovereign facility"
			role="img"
		/>
	{/if}

	<Text
		x={width / 2}
		y={height - 6}
		anchor="middle"
		fill="var(--color-text)"
		fontSize={11}
		fontWeight={600}>{name}</Text
	>
</svg>

{#if caption}
	<div class="dc-caption">{caption}</div>
{/if}

<style>
	.dc-interactive {
		cursor: pointer;
	}
	.dc-caption {
		font-size: 0.72rem;
		color: var(--color-text-subtle);
		text-align: center;
		margin-top: 0.25rem;
	}
</style>