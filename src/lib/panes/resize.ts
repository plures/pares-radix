/**
 * Split-pane resize math — pure, framework-free.
 *
 * A split has a primary child of `size` px and a secondary child of
 * `total - size` px, separated by a sash. Resizing must respect the minimum
 * size of each child (`minA` for primary, `minB` for secondary) and the total.
 */

export interface ResizeParams {
	/** Total available px along the resize axis (both children + implied sash). */
	total: number;
	/** Minimum px for the primary (A) child. */
	minA: number;
	/** Minimum px for the secondary (B) child. */
	minB: number;
}

/**
 * Clamp a proposed primary-child size to the legal range given both minimums
 * and the total. Guarantees minA <= result <= total - minB when feasible; if
 * the range is infeasible (minA + minB > total) it collapses to the midpoint.
 */
export function clampSize(proposed: number, params: ResizeParams): number {
	const { total, minA, minB } = params;
	const max = total - minB;
	if (minA > max) {
		// Infeasible range — best-effort: clamp to feasible midpoint.
		const mid = total / 2;
		return Math.max(0, Math.min(total, mid));
	}
	if (proposed < minA) return minA;
	if (proposed > max) return max;
	return proposed;
}

/**
 * Apply a pointer delta (px) to a starting primary size, then clamp.
 * Positive delta grows the primary child.
 */
export function applyDelta(startSize: number, deltaPx: number, params: ResizeParams): number {
	return clampSize(startSize + deltaPx, params);
}

export type ResizeKey =
	| 'ArrowLeft'
	| 'ArrowRight'
	| 'ArrowUp'
	| 'ArrowDown'
	| 'Home'
	| 'End';

/**
 * Keyboard resize. ArrowLeft/ArrowUp shrink the primary child by `step`;
 * ArrowRight/ArrowDown grow it. Home clamps to minimum primary; End clamps to
 * maximum primary (i.e. minimum secondary). Unknown keys return current.
 */
export function keyResize(
	current: number,
	key: ResizeKey | string,
	step: number,
	params: ResizeParams
): number {
	switch (key) {
		case 'ArrowLeft':
		case 'ArrowUp':
			return clampSize(current - step, params);
		case 'ArrowRight':
		case 'ArrowDown':
			return clampSize(current + step, params);
		case 'Home':
			return clampSize(params.minA, params);
		case 'End':
			return clampSize(params.total - params.minB, params);
		default:
			return current;
	}
}
