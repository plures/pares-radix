/**
 * graph-layout.ts — pure, framework-free layout engine for `GraphView` (ADR-0032).
 *
 * This module owns the hard part of ADR-0032 §2.2 ("graph-flex"): the space-budget
 * allocation pass. It is deliberately free of Svelte / DOM so it can be unit-tested
 * deterministically at fixed container sizes (phone-portrait, tablet, wide-desktop).
 *
 * Responsibilities:
 *   1. Center-first allocation — the focus node gets priority space + detail.
 *   2. Angular stub distribution weighted by container aspect ratio (one control,
 *      no per-breakpoint code): wide containers spread horizontally, narrow stack
 *      vertically.
 *   3. Space budget → DetailLevel (auto-summarization); center protected, outer
 *      stubs demote first.
 *   4. High-degree collapse: excess edges beyond a cap fold into an "N more" affordance.
 *   5. Independent expansion with minimum-aware reflow: expanding one node only
 *      reflows/demotes neighbors that would be forced below `minNodeSize`; neighbors
 *      that stay above their minimum are left untouched (no global relayout jitter).
 *
 * No state is owned here — every call is a pure function of its inputs.
 */

import type {
	GraphNeighborhood,
	GraphNode,
	DetailLevel,
	SpaceBudget,
	MinNodeSize,
} from './types-local.js';

export interface ContainerSize {
	width: number;
	height: number;
}

/** A node's computed placement in the ego-centric layout. */
export interface NodePlacement {
	nodeId: string;
	/** true for the focus node (always centered). */
	isFocus: boolean;
	/** center-point x in px, container-relative. */
	x: number;
	/** center-point y in px, container-relative. */
	y: number;
	/** granted box width in px. */
	w: number;
	/** granted box height in px. */
	h: number;
	/** angle (radians) of the stub around the focus; 0 for the focus itself. */
	angle: number;
	/** detail level granted from the box (auto-summarization). */
	detail: DetailLevel;
	/** true when this node was forced to reflow by a neighbor's expansion. */
	reflowed: boolean;
}

/** Full result of a layout pass. */
export interface LayoutResult {
	focusId: string;
	container: ContainerSize;
	/** aspect ratio (w/h) of the container. */
	aspect: number;
	/** placements for the focus + every rendered stub. */
	placements: NodePlacement[];
	/** rendered stub count (after high-degree cap). */
	renderedStubs: number;
	/** number of edges/neighbors collapsed into the "N more" affordance (0 = none). */
	collapsedCount: number;
}

export interface LayoutOptions {
	/** container in px. */
	container: ContainerSize;
	/** zoom multiplier (>= 0.25). Feeds the space budget (ADR-0032 §2.2.5). */
	zoom?: number;
	/** the "minimum" box that governs neighbor reflow (ADR-0032 §2.2.4). */
	minNodeSize?: MinNodeSize;
	/**
	 * node id that is currently expanded (grows its target box). Only neighbors
	 * forced below `minNodeSize` by this expansion reflow.
	 */
	expandedId?: string | null;
	/** optional host override of the auto-summarizer. */
	detailFor?: (node: GraphNode, space: SpaceBudget) => DetailLevel;
}

export const DEFAULT_MIN_NODE_SIZE: MinNodeSize = { w: 88, h: 44 };

/** Hard cap on rendered stubs before excess collapses to "N more". */
const MAX_STUBS = 12;

/** Detail-level thresholds keyed on the smaller granted box dimension (px). */
const DETAIL_THRESHOLDS: { level: DetailLevel; minArea: number }[] = [
	{ level: 'full', minArea: 160 * 120 },
	{ level: 'title+keyFields', minArea: 120 * 72 },
	{ level: 'title', minArea: 72 * 36 },
	{ level: 'icon', minArea: 0 },
];

/** Map a granted box to a DetailLevel (auto-summarization). */
export function detailLevelForBox(w: number, h: number): DetailLevel {
	const area = Math.max(0, w) * Math.max(0, h);
	for (const t of DETAIL_THRESHOLDS) {
		if (area >= t.minArea) return t.level;
	}
	return 'icon';
}

/**
 * Weight the angular spread by aspect ratio. Returns a per-stub angle so that a
 * wide container biases stubs toward the horizontal axis and a tall/narrow one
 * biases toward the vertical axis. This is the flexbox analogue: one control,
 * container-driven, no breakpoint forks (ADR-0032 §2.2.2).
 */
export function stubAngles(count: number, aspect: number): number[] {
	if (count <= 0) return [];
	if (count === 1) {
		// single stub: place along the dominant axis
		return [aspect >= 1 ? 0 : Math.PI / 2];
	}
	const angles: number[] = [];
	// Even base distribution around the circle.
	for (let i = 0; i < count; i++) {
		const base = (2 * Math.PI * i) / count - Math.PI / 2; // start at top
		// Aspect warp: compress angles toward the container's long axis.
		// warp in (-1..1): >0 wide -> pull toward horizontal, <0 tall -> pull toward vertical.
		const warp = (aspect - 1) / (aspect + 1);
		const warped = warpAngle(base, warp);
		angles.push(warped);
	}
	return angles;
}

/**
 * Warp an angle toward the horizontal (warp>0) or vertical (warp<0) axis.
 * warp === 0 is an identity (square container -> even radial spread).
 */
function warpAngle(angle: number, warp: number): number {
	if (warp === 0) return normalizeAngle(angle);
	// Decompose into a unit vector, scale the minor-axis component, re-derive angle.
	let cx = Math.cos(angle);
	let cy = Math.sin(angle);
	if (warp > 0) {
		// wide: shrink vertical component -> pull toward horizontal
		cy *= 1 - warp * 0.7;
	} else {
		// tall: shrink horizontal component -> pull toward vertical
		cx *= 1 + warp * 0.7;
	}
	return normalizeAngle(Math.atan2(cy, cx));
}

function normalizeAngle(a: number): number {
	let x = a;
	while (x <= -Math.PI) x += 2 * Math.PI;
	while (x > Math.PI) x -= 2 * Math.PI;
	return x;
}

/**
 * Compute the ego-centric layout for a neighborhood at a given container size.
 * Pure: no DOM, no side effects.
 */
export function computeLayout(
	neighborhood: GraphNeighborhood,
	opts: LayoutOptions,
): LayoutResult {
	const zoom = Math.max(0.25, opts.zoom ?? 1);
	const min = opts.minNodeSize ?? DEFAULT_MIN_NODE_SIZE;
	const container = opts.container;
	const width = Math.max(1, container.width) * zoom;
	const height = Math.max(1, container.height) * zoom;
	const aspect = width / height;
	const cx = width / 2;
	const cy = height / 2;

	const focus =
		neighborhood.nodes.find((n) => n.id === neighborhood.focusId) ??
		neighborhood.nodes[0];
	if (!focus) {
		return {
			focusId: neighborhood.focusId,
			container,
			aspect,
			placements: [],
			renderedStubs: 0,
			collapsedCount: 0,
		};
	}

	const stubsAll = neighborhood.nodes.filter((n) => n.id !== focus.id);

	// High-degree collapse: cap rendered stubs; the rest fold into "N more".
	const renderedStubNodes = stubsAll.slice(0, MAX_STUBS);
	const collapsedCount = stubsAll.length - renderedStubNodes.length;

	// --- 1. Center-first allocation ------------------------------------------
	// Focus gets the largest box that fits comfortably in the center, reserving a
	// margin ring for stubs. Expansion of the focus grants it more.
	const focusExpanded = opts.expandedId === focus.id;
	const focusScale = focusExpanded ? 0.42 : 0.32;
	const focusW = clamp(width * focusScale, min.w, width * 0.6);
	const focusH = clamp(height * focusScale, min.h, height * 0.6);

	const placements: NodePlacement[] = [];
	placements.push({
		nodeId: focus.id,
		isFocus: true,
		x: cx,
		y: cy,
		w: focusW,
		h: focusH,
		angle: 0,
		detail: opts.detailFor
			? opts.detailFor(focus, { w: focusW, h: focusH, zoom })
			: detailLevelForBox(focusW, focusH),
		reflowed: false,
	});

	// --- 2. Radial stub placement --------------------------------------------
	const count = renderedStubNodes.length;
	const angles = stubAngles(count, aspect);

	// Ring radius: outside the focus box, inside the container. Weighted per-axis
	// so wide containers push stubs further horizontally.
	const ringX = Math.max(focusW * 0.6 + min.w * 0.6, (width - min.w) / 2 - 8);
	const ringY = Math.max(focusH * 0.6 + min.h * 0.6, (height - min.h) / 2 - 8);

	// Base stub box: share the outer budget among stubs; shrinks as degree rises.
	const degreePenalty = Math.max(1, count / 4);
	const baseStubW = clamp((width * 0.22) / degreePenalty, min.w, width * 0.3);
	const baseStubH = clamp((height * 0.18) / degreePenalty, min.h, height * 0.3);

	const expandedId = opts.expandedId && opts.expandedId !== focus.id ? opts.expandedId : null;

	for (let i = 0; i < count; i++) {
		const node = renderedStubNodes[i];
		const angle = angles[i];
		const isExpanded = expandedId === node.id;

		// --- 4. Independent expansion with minimum-aware reflow --------------
		// The expanded stub grows. Its immediate ring-neighbors reflow ONLY if the
		// growth would force them below min; otherwise they are untouched.
		let w = baseStubW;
		let h = baseStubH;
		let reflowed = false;

		if (isExpanded) {
			w = clamp(baseStubW * 1.8, min.w, width * 0.4);
			h = clamp(baseStubH * 1.8, min.h, height * 0.4);
		} else if (expandedId) {
			// Is this stub an angular neighbor of the expanded stub?
			const expIdx = renderedStubNodes.findIndex((n) => n.id === expandedId);
			const adjacency = ringNeighborDistance(i, expIdx, count);
			if (adjacency === 1) {
				// Would keeping base size overlap the grown node? Compute the space
				// this neighbor must cede. Only shrink if the ceded box stays >= min;
				// if shrinking would drop below min, it demotes (reflows) to min.
				const needed = neededShrink(count, aspect);
				const shrunkW = baseStubW * needed;
				const shrunkH = baseStubH * needed;
				if (shrunkW < min.w || shrunkH < min.h) {
					// minimum violated -> reflow to the minimum box + mark reflowed
					w = min.w;
					h = min.h;
					reflowed = true;
				} else {
					// stays above minimum -> untouched (ADR-0032: no reflow)
					w = baseStubW;
					h = baseStubH;
				}
			}
		}

		const x = cx + Math.cos(angle) * ringX * stubRadiusScale(w, ringX);
		const y = cy + Math.sin(angle) * ringY * stubRadiusScale(h, ringY);

		// clamp inside container
		const px = clamp(x, w / 2, width - w / 2);
		const py = clamp(y, h / 2, height - h / 2);

		placements.push({
			nodeId: node.id,
			isFocus: false,
			x: px,
			y: py,
			w,
			h,
			angle,
			detail: opts.detailFor
				? opts.detailFor(node, { w, h, zoom })
				: detailLevelForBox(w, h),
			reflowed,
		});
	}

	return {
		focusId: focus.id,
		container,
		aspect,
		placements,
		renderedStubs: count,
		collapsedCount,
	};
}

/** Keep the stub center a touch inside the ring so its box fits. */
function stubRadiusScale(box: number, ring: number): number {
	if (ring <= 0) return 1;
	const inset = box / (2 * ring);
	return clamp(1 - inset * 0.5, 0.4, 1);
}

/** Fractional retained size a base-neighbor keeps when an adjacent node expands. */
function neededShrink(count: number, aspect: number): number {
	// More stubs -> tighter ring -> more shrink pressure. Wide containers relieve
	// horizontal pressure. Bounded to (0.5..0.95).
	const pressure = Math.min(1, count / MAX_STUBS);
	const relief = aspect >= 1 ? 0.15 : 0;
	return clamp(0.95 - pressure * 0.5 + relief, 0.5, 0.95);
}

/** Ring adjacency distance between two stub indices on a cyclic ring. */
function ringNeighborDistance(i: number, j: number, count: number): number {
	if (j < 0 || count <= 1) return Infinity;
	const d = Math.abs(i - j);
	return Math.min(d, count - d);
}

function clamp(v: number, lo: number, hi: number): number {
	if (hi < lo) return lo;
	return Math.min(hi, Math.max(lo, v));
}

/** Axis-aligned box overlap test on two placements (for tests + overlap guards). */
export function placementsOverlap(a: NodePlacement, b: NodePlacement): boolean {
	const ax0 = a.x - a.w / 2;
	const ax1 = a.x + a.w / 2;
	const ay0 = a.y - a.h / 2;
	const ay1 = a.y + a.h / 2;
	const bx0 = b.x - b.w / 2;
	const bx1 = b.x + b.w / 2;
	const by0 = b.y - b.h / 2;
	const by1 = b.y + b.h / 2;
	const EPS = 0.5;
	return ax0 < bx1 - EPS && ax1 > bx0 + EPS && ay0 < by1 - EPS && ay1 > by0 + EPS;
}
