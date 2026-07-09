/**
 * operations-view — pure view-model helpers for the Operations scene.
 *
 * Extracted from the route so the mapping from constraint-checked fleet facts
 * to representational shapes (racks, region datacenters, beacons) is unit
 * testable without a browser (C-TEST-002). The route stays a pure projection.
 */

import type { ServiceRecord } from "./operations.js";
import type { RackUnit, BeaconStatus, RegionTone } from "@plures/design-dojo";

/** Service health rollup -> status-lamp colour. */
export function beacon(h: ServiceRecord["health"]): BeaconStatus {
	return h === "healthy" ? "healthy" : h === "degraded" ? "warning" : "critical";
}

/**
 * A service becomes a rack: one sled per replica. Sleds at/below the SLO floor
 * (minReplicas) are load-bearing and take the service health; sleds above it are
 * spare capacity shown as idle. Load LEDs reflect health pressure.
 */
export function unitsFor(s: ServiceRecord): RackUnit[] {
	return Array.from({ length: s.replicas }, (_, i) => ({
		id: `${s.id}-r${i}`,
		label: `${s.name.split("-").pop()}-${String(i + 1).padStart(2, "0")}`,
		status: (i >= s.minReplicas ? "idle" : beacon(s.health)) as BeaconStatus,
		load: s.health === "healthy" ? 0.35 + (i % 3) * 0.2 : s.health === "degraded" ? 0.85 : 1,
	}));
}

export interface RegionGroup {
	code: string;
	services: ServiceRecord[];
}

/** Group the flat fleet into regions, preserving first-seen order. */
export function groupByRegion(fleet: ServiceRecord[]): RegionGroup[] {
	const map = new Map<string, ServiceRecord[]>();
	for (const s of fleet) {
		if (!map.has(s.region)) map.set(s.region, []);
		map.get(s.region)!.push(s);
	}
	return [...map.entries()].map(([code, services]) => ({ code, services }));
}

/** Worst-health-wins tone for a region. */
export function regionTone(services: ServiceRecord[]): RegionTone {
	if (services.some((s) => s.health === "breaching")) return "critical";
	if (services.some((s) => s.health === "degraded")) return "warning";
	return "healthy";
}

export function regionMetric(services: ServiceRecord[]): string {
	const replicas = services.reduce((n, s) => n + s.replicas, 0);
	const avg = services.reduce((n, s) => n + s.errorBudget, 0) / Math.max(1, services.length);
	return `${services.length} svc \u00b7 ${replicas} replicas \u00b7 ${Math.round(avg * 100)}% budget`;
}

/** Datacenter window grid = one lit window per replica, coloured by health. */
export function regionWindows(services: ServiceRecord[]): BeaconStatus[] {
	return services.flatMap((s) =>
		Array.from({ length: s.replicas }, (_, i) =>
			i >= s.minReplicas ? ("idle" as BeaconStatus) : beacon(s.health),
		),
	);
}

/** usgovvirginia -> "US Gov Virginia". Known Azure-style region codes. */
export function prettyRegion(code: string): string {
	const known: Record<string, string> = {
		usgovvirginia: "US Gov Virginia",
		usgovarizona: "US Gov Arizona",
		usgovtexas: "US Gov Texas",
		eastus: "East US",
		eastus2: "East US 2",
		westus: "West US",
		westus2: "West US 2",
		centralus: "Central US",
	};
	if (known[code]) return known[code];
	// Fallback: split a lowercase code into title-cased words.
	const withGov = code.replace(/^usgov/, "us gov ").replace(/^us(?=\w)/, "us ");
	return withGov
		.replace(/([a-z])(\d)/g, "$1 $2")
		.split(/\s+/)
		.map((w) => (w === "us" ? "US" : w.charAt(0).toUpperCase() + w.slice(1)))
		.join(" ")
		.trim();
}