<!-- @component
  PluginModule — a plugin drawn as a physical expansion module/cartridge that
  seats into a bay: connector pins, a status lamp, a heat-sink ridge and a
  label plate. Active = seated (pins lit); inactive = ejected (raised, dim).
  Health lamp colour follows the plugin status. Replaces flat plugin cards.

  Usage:
    <PluginModule name="AI Canvas" version="0.1.0" status="healthy" active />
-->
<script lang="ts">
	import { Rect, Group, Text } from "@plures/design-dojo-npm/svg";
	import StatusBeacon from "./StatusBeacon.svelte";
	import type { BeaconStatus } from "../types-local.js";

	interface Props {
		name: string;
		version?: string;
		status?: BeaconStatus;
		/** Seated (true) vs ejected (false). */
		active?: boolean;
		/** Number of surfaces/capabilities → connector-pin count hint. */
		surfaces?: number;
		width?: number;
		onclick?: () => void;
	}

	let {
		name,
		version,
		status = "unknown",
		active = true,
		surfaces = 4,
		width = 170,
		onclick,
	}: Props = $props();

	const height = 78;
	let pinCount = $derived(Math.max(4, Math.min(10, surfaces * 2)));
	let seatY = $derived(active ? 6 : 0);
	let pinFill = $derived(active ? "var(--color-accent)" : "var(--color-bg-active)");
</script>

<svg
	{width}
	{height}
	viewBox={`0 0 ${width} ${height}`}
	role={onclick ? "button" : "group"}
	aria-label={`plugin ${name} ${active ? "active" : "inactive"}`}
	{onclick}
	class={onclick ? "mod-interactive" : undefined}
>
	<!-- bay slot (recessed) -->
	<Rect
		x={2}
		y={height - 20}
		width={width - 4}
		height={18}
		rx={3}
		fill="var(--color-bg)"
		stroke="var(--color-border-muted)"
		strokeWidth={1}
	/>

	<Group translateY={seatY}>
		<!-- connector pins -->
		<Group translateX={(width - (pinCount * 8 - 3)) / 2} translateY={height - 22}>
			{#each Array(pinCount) as _, k (k)}
				<Rect x={k * 8} y={0} width={5} height={9} rx={1} fill={pinFill} opacity={active ? 1 : 0.5} />
			{/each}
		</Group>

		<!-- module PCB body -->
		<Rect
			x={6}
			y={4}
			width={width - 12}
			height={height - 30}
			rx={5}
			fill="var(--color-bg-card)"
			stroke={active ? "var(--color-border)" : "var(--color-border-muted)"}
			strokeWidth={1.4}
			opacity={active ? 1 : 0.75}
		/>
		<!-- heat-sink ridges -->
		{#each Array(5) as _, r (r)}
			<Rect
				x={width - 44 + r * 7}
				y={10}
				width={3}
				height={height - 42}
				rx={1}
				fill="var(--color-bg-active)"
			/>
		{/each}

		<!-- status lamp -->
		<StatusBeacon {status} x={20} y={18} r={5} />
		<!-- label plate -->
		<Text x={34} y={17} fill="var(--color-text)" fontSize={12} fontWeight={600}>{name}</Text>
		{#if version}
			<Text x={34} y={31} fill="var(--color-text-subtle)" fontSize={9}>v{version}</Text>
		{/if}
	</Group>
</svg>

<style>
	.mod-interactive {
		cursor: pointer;
	}
</style>