<!-- @component
  BeaconBadge — a standalone status lamp with its own <svg> canvas, for use
  OUTSIDE an existing SVG context (e.g. inline in a header or a readiness row).
  Wraps the StatusBeacon primitive so app code never hand-rolls raw <svg>.

  Usage:
    <BeaconBadge status="healthy" size={28} />
-->
<script lang="ts">
	import StatusBeacon from "./StatusBeacon.svelte";
	import type { BeaconStatus } from "./StatusBeacon.svelte";

	interface Props {
		status?: BeaconStatus;
		/** Canvas edge length in px. Default 24. */
		size?: number;
		pulse?: boolean;
		"aria-label"?: string;
	}

	let { status = "unknown", size = 24, pulse = true, ...rest }: Props = $props();
	let label = $derived((rest["aria-label"] as string | undefined) ?? `status: ${status}`);
	let r = $derived(size * 0.26);
</script>

<svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} role="img" aria-label={label}>
	<StatusBeacon {status} x={size / 2} y={size / 2} {r} {pulse} />
</svg>