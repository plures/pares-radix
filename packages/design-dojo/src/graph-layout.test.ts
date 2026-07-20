import { describe, it, expect } from 'vitest';
import {
	computeLayout,
	detailLevelForBox,
	stubAngles,
	placementsOverlap,
	DEFAULT_MIN_NODE_SIZE,
	type NodePlacement,
} from './graph-layout.js';
import type { GraphNeighborhood, GraphNode } from './types-local.js';

// --- Fixtures ---------------------------------------------------------------

function node(id: string, extra: Partial<GraphNode> = {}): GraphNode {
	return { id, label: `Node ${id}`, ...extra };
}

/** Build a focus + `degree` neighbor stubs, all edge-linked to the focus. */
function neighborhood(degree: number): GraphNeighborhood {
	const nodes: GraphNode[] = [node('focus', { type: 'root', fields: { a: 1, b: 2, c: 3 } })];
	const edges = [];
	for (let i = 0; i < degree; i++) {
		const id = `n${i}`;
		nodes.push(node(id, { fields: { x: i } }));
		edges.push({ id: `e${i}`, from: 'focus', to: id, label: `rel${i}` });
	}
	return { focusId: 'focus', nodes, edges };
}

const PHONE = { width: 390, height: 844 };
const TABLET = { width: 834, height: 1112 };
const WIDE = { width: 2560, height: 1440 };

function findFocus(placements: NodePlacement[]): NodePlacement {
	const f = placements.find((p) => p.isFocus);
	if (!f) throw new Error('no focus placement');
	return f;
}

function anyOverlap(placements: NodePlacement[]): boolean {
	for (let i = 0; i < placements.length; i++) {
		for (let j = i + 1; j < placements.length; j++) {
			if (placementsOverlap(placements[i], placements[j])) return true;
		}
	}
	return false;
}

// --- detailLevelForBox ------------------------------------------------------

describe('detailLevelForBox (auto-summarization)', () => {
	it('grants full detail for a large box', () => {
		expect(detailLevelForBox(220, 160)).toBe('full');
	});
	it('demotes progressively as the box shrinks', () => {
		expect(detailLevelForBox(130, 80)).toBe('title+keyFields');
		expect(detailLevelForBox(90, 40)).toBe('title');
		expect(detailLevelForBox(20, 20)).toBe('icon');
	});
});

// --- stubAngles (aspect-weighted distribution) ------------------------------

describe('stubAngles (aspect-weighted angular distribution)', () => {
	it('a wide container biases stubs toward the horizontal axis', () => {
		const wide = stubAngles(6, 2560 / 1440); // aspect ~1.78
		const square = stubAngles(6, 1);
		// mean absolute cos should be larger (more horizontal) for the wide container
		const meanCos = (a: number[]) =>
			a.reduce((s, x) => s + Math.abs(Math.cos(x)), 0) / a.length;
		expect(meanCos(wide)).toBeGreaterThan(meanCos(square));
	});

	it('a narrow/tall container biases stubs toward the vertical axis', () => {
		const tall = stubAngles(6, 390 / 844); // aspect ~0.46
		const square = stubAngles(6, 1);
		const meanSin = (a: number[]) =>
			a.reduce((s, x) => s + Math.abs(Math.sin(x)), 0) / a.length;
		expect(meanSin(tall)).toBeGreaterThan(meanSin(square));
	});

	it('a single stub lands on the dominant axis', () => {
		expect(Math.abs(Math.cos(stubAngles(1, 2)[0]))).toBeCloseTo(1); // wide -> horizontal
		expect(Math.abs(Math.sin(stubAngles(1, 0.5)[0]))).toBeCloseTo(1); // tall -> vertical
	});
});

// --- center-first + non-overlap at three fixed sizes ------------------------

describe.each([
	['phone-portrait', PHONE],
	['tablet', TABLET],
	['wide-desktop', WIDE],
])('layout at %s', (_name, container) => {
	it('centers the focus node', () => {
		const r = computeLayout(neighborhood(5), { container });
		const focus = findFocus(r.placements);
		// container is scaled by zoom(=1); center is width/2,height/2
		expect(focus.x).toBeCloseTo(container.width / 2, 0);
		expect(focus.y).toBeCloseTo(container.height / 2, 0);
	});

	it('grants the focus the largest box (center prioritized)', () => {
		const r = computeLayout(neighborhood(6), { container });
		const focus = findFocus(r.placements);
		const stubs = r.placements.filter((p) => !p.isFocus);
		const focusArea = focus.w * focus.h;
		for (const s of stubs) {
			expect(focusArea).toBeGreaterThanOrEqual(s.w * s.h);
		}
	});

	it('produces non-overlapping placements for a moderate degree', () => {
		const r = computeLayout(neighborhood(6), { container });
		expect(anyOverlap(r.placements)).toBe(false);
	});

	it('keeps focus detail protected (>= any stub detail level)', () => {
		const order = { icon: 0, title: 1, 'title+keyFields': 2, full: 3 } as const;
		const r = computeLayout(neighborhood(8), { container });
		const focus = findFocus(r.placements);
		const stubs = r.placements.filter((p) => !p.isFocus);
		for (const s of stubs) {
			expect(order[focus.detail]).toBeGreaterThanOrEqual(order[s.detail]);
		}
	});
});

// --- high-degree collapse ("N more") ----------------------------------------

describe('high-degree collapse', () => {
	it('collapses excess neighbors beyond the cap into a count', () => {
		const r = computeLayout(neighborhood(30), { container: WIDE });
		expect(r.renderedStubs).toBeLessThanOrEqual(12);
		expect(r.collapsedCount).toBe(30 - r.renderedStubs);
		expect(r.collapsedCount).toBeGreaterThan(0);
	});

	it('does not collapse when degree is under the cap', () => {
		const r = computeLayout(neighborhood(5), { container: WIDE });
		expect(r.collapsedCount).toBe(0);
		expect(r.renderedStubs).toBe(5);
	});
});

// --- minimum-aware reflow on expansion --------------------------------------

describe('minimum-aware reflow (independent expansion)', () => {
	it('leaves above-minimum neighbors untouched when a node expands', () => {
		const nb = neighborhood(6);
		const min = DEFAULT_MIN_NODE_SIZE;
		const base = computeLayout(nb, { container: WIDE, minNodeSize: min });
		const expanded = computeLayout(nb, {
			container: WIDE,
			minNodeSize: min,
			expandedId: 'n0',
		});

		const byId = (r: typeof base, id: string) =>
			r.placements.find((p) => p.nodeId === id)!;

		// The expanded node grew.
		expect(byId(expanded, 'n0').w).toBeGreaterThan(byId(base, 'n0').w);

		// Non-adjacent neighbors must be byte-identical in size (no jitter).
		const nonReflowed = expanded.placements.filter((p) => !p.isFocus && !p.reflowed && p.nodeId !== 'n0');
		expect(nonReflowed.length).toBeGreaterThan(0);
		for (const p of nonReflowed) {
			const b = byId(base, p.nodeId);
			expect(p.w).toBeCloseTo(b.w, 5);
			expect(p.h).toBeCloseTo(b.h, 5);
		}
	});

	it('reflows only minimum-violating neighbors (flagged reflowed)', () => {
		// High degree + tight phone container -> expansion forces adjacent neighbors
		// below minimum, which must reflow rather than overlap.
		const nb = neighborhood(10);
		const expanded = computeLayout(nb, {
			container: PHONE,
			minNodeSize: { w: 100, h: 60 },
			expandedId: 'n0',
		});
		const reflowed = expanded.placements.filter((p) => p.reflowed);
		// Reflowed nodes sit exactly at the minimum box.
		for (const p of reflowed) {
			expect(p.w).toBe(100);
			expect(p.h).toBe(60);
		}
		// Focus is never reflowed.
		expect(findFocus(expanded.placements).reflowed).toBe(false);
	});

	it('expanding the focus grows the center box', () => {
		const nb = neighborhood(4);
		const base = findFocus(computeLayout(nb, { container: WIDE }).placements);
		const exp = findFocus(computeLayout(nb, { container: WIDE, expandedId: 'focus' }).placements);
		expect(exp.w).toBeGreaterThan(base.w);
	});
});

// --- zoom feeds the space budget --------------------------------------------

describe('zoom → space budget', () => {
	it('zooming in grants larger boxes / richer detail', () => {
		const nb = neighborhood(6);
		const inFocus = findFocus(computeLayout(nb, { container: PHONE, zoom: 2 }).placements);
		const outFocus = findFocus(computeLayout(nb, { container: PHONE, zoom: 1 }).placements);
		expect(inFocus.w).toBeGreaterThan(outFocus.w);
	});
});

// --- empty / degenerate -----------------------------------------------------

describe('degenerate inputs', () => {
	it('handles a focus with no neighbors', () => {
		const r = computeLayout({ focusId: 'focus', nodes: [node('focus')], edges: [] }, { container: TABLET });
		expect(r.placements).toHaveLength(1);
		expect(r.renderedStubs).toBe(0);
		expect(r.collapsedCount).toBe(0);
	});

	it('handles an empty neighborhood without throwing', () => {
		const r = computeLayout({ focusId: 'x', nodes: [], edges: [] }, { container: TABLET });
		expect(r.placements).toHaveLength(0);
	});
});
