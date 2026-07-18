<!-- @component
  StatusBeacon — a semantic status indicator that reads as a physical "light".

  A glowing lamp/LED, not a flat badge. The colour + glow encode urgency so the
  eye is drawn to trouble, the way an amber/red light on real hardware does.
  Composed from the design-dojo SVG Circle primitive (token-compliant fills);
  the optional pulse is a CSS animation on the lamp core.

  Usage (inside an <svg>):
    <StatusBeacon status="critical" x={12} y={12} />
-->
<script lang="ts">
	import { Circle } from "@plures/design-dojo-npm/svg";
	import type { SvgFill } from "@plures/design-dojo-npm/svg";
	import type { BeaconStatus } from "../types-local.js";

	interface Props {
		status?: BeaconStatus;
		x?: number;
		y?: number;
		r?: number;
		pulse?: boolean;
		"aria-label"?: string;
	}

	let { status = "unknown", x = 0, y = 0, r = 5, pulse = true, ...rest }: Props = $props();

	const fillFor: Record<BeaconStatus, SvgFill> = {
		healthy: "var(--color-success)",
		warning: "var(--color-warning)",
		critical: "var(--color-danger)",
		idle: "var(--color-text-subtle)",
		unknown: "var(--color-border)",
	};

	let fill = $derived(fillFor[status]);
	let animate = $derived(pulse && (status === "critical" || status === "warning"));
	let label = $derived((rest["aria-label"] as string | undefined) ?? `status: ${status}`);
</script>

<!-- outer glow + halo via the compliant Circle primitive -->
<Circle cx={x} cy={y} r={r * 2.1} {fill} opacity={0.16} aria-label={label} role="img" />
<Circle cx={x} cy={y} r={r * 1.4} {fill} opacity={0.34} />

<!-- lamp core (plain circle so it can carry the pulse animation) -->
<circle
	cx={x}
	cy={y}
	{r}
	{fill}
	class={animate ? "beacon-core beacon-pulse" : "beacon-core"}
/>
<!-- specular highlight -->
<Circle cx={x - r * 0.3} cy={y - r * 0.3} r={r * 0.35} fill="var(--color-text)" opacity={0.5} />

<style>
	.beacon-pulse {
		animation: beacon-breathe 1.4s ease-in-out infinite;
		transform-box: fill-box;
		transform-origin: center;
	}
	@keyframes beacon-breathe {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.4;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.beacon-pulse {
			animation: none;
		}
	}
</style>