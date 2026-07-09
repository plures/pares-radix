/**
 * operations-view — logic tests (C-TEST-002: channel-independent, no browser).
 *
 * Pins the mapping from constraint-checked fleet facts to the representational
 * shapes: services -> racks (sleds per replica), regions -> datacenter windows,
 * health -> beacon colour. Uses the real seedFleet as the fixture so the test
 * tracks the actual demo scene.
 */

import { describe, it, expect } from "vitest";
import { seedFleet, type ServiceRecord } from "./operations.js";
import {
	beacon,
	unitsFor,
	groupByRegion,
	regionTone,
	regionMetric,
	regionWindows,
	prettyRegion,
} from "./operations-view.js";

const web = seedFleet.find((s) => s.id === "svc-web")!;
const identity = seedFleet.find((s) => s.id === "svc-identity")!; // degraded, replicas===minReplicas

describe("operations-view — health → beacon", () => {
	it("maps the three health states", () => {
		expect(beacon("healthy")).toBe("healthy");
		expect(beacon("degraded")).toBe("warning");
		expect(beacon("breaching")).toBe("critical");
	});
});

describe("operations-view — service → rack units", () => {
	it("emits one sled per replica", () => {
		expect(unitsFor(web)).toHaveLength(web.replicas);
	});
	it("sleds above the SLO floor are spare (idle); load-bearing take health", () => {
		const units = unitsFor(web); // 6 replicas, min 4, healthy
		const loadBearing = units.slice(0, web.minReplicas);
		const spare = units.slice(web.minReplicas);
		expect(loadBearing.every((u) => u.status === "healthy")).toBe(true);
		expect(spare.every((u) => u.status === "idle")).toBe(true);
	});
	it("a degraded service lights its load-bearing sleds amber", () => {
		const units = unitsFor(identity); // 3/3 degraded → all load-bearing
		expect(units.every((u) => u.status === "warning")).toBe(true);
		expect(units.every((u) => (u.load ?? 0) >= 0.85)).toBe(true);
	});
	it("unit labels are stable + unique", () => {
		const ids = unitsFor(web).map((u) => u.id);
		expect(new Set(ids).size).toBe(ids.length);
	});
});

describe("operations-view — region grouping", () => {
	it("groups the seed fleet by region, order-preserving", () => {
		const groups = groupByRegion(seedFleet);
		expect(groups.map((g) => g.code)).toEqual(["usgovvirginia", "usgovarizona"]);
		expect(groups[0].services.map((s) => s.id)).toEqual(["svc-web", "svc-ledger"]);
	});
	it("region tone is worst-health-wins", () => {
		const az = groupByRegion(seedFleet).find((g) => g.code === "usgovarizona")!;
		expect(regionTone(az.services)).toBe("warning"); // identity-broker degraded
		const va = groupByRegion(seedFleet).find((g) => g.code === "usgovvirginia")!;
		expect(regionTone(va.services)).toBe("healthy");
	});
	it("region metric summarises svc / replicas / avg budget", () => {
		const va = groupByRegion(seedFleet).find((g) => g.code === "usgovvirginia")!;
		expect(regionMetric(va.services)).toMatch(/2 svc · 10 replicas · \d+% budget/);
	});
	it("datacenter windows = one per replica across the region", () => {
		const az = groupByRegion(seedFleet).find((g) => g.code === "usgovarizona")!;
		const total = az.services.reduce((n, s) => n + s.replicas, 0);
		expect(regionWindows(az.services)).toHaveLength(total);
	});
});

describe("operations-view — region labels", () => {
	it("prettifies cloud region codes", () => {
		expect(prettyRegion("usgovvirginia")).toBe("US Gov Virginia");
		expect(prettyRegion("westus2")).toContain("US");
	});
});

describe("operations-view — breaching escalates tone", () => {
	it("a breaching service turns the region critical", () => {
		const svc: ServiceRecord = { ...web, health: "breaching" };
		expect(regionTone([svc])).toBe("critical");
		expect(regionWindows([svc]).slice(0, svc.minReplicas).every((w) => w === "critical")).toBe(true);
	});
});