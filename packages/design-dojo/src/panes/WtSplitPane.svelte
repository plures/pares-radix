<script lang="ts">
	/**
	 * WtSplitPane — two-child resizable split with a real, keyboard-accessible sash.
	 *
	 * The sash is a role="separator" element supporting pointer drag (with pointer
	 * capture) and Arrow/Home/End keyboard resize. `size` is the primary child's
	 * px extent along the split axis and is $bindable. The caller persists size /
	 * collapse via the onresize / oncollapse callbacks — no store is baked in.
	 */
	import type { Snippet } from 'svelte';
	import type { Orientation } from '../../../../src/lib/panes/types.js';
	import { applyDelta, keyResize, type ResizeParams } from '../../../../src/lib/panes/resize.js';

	interface Props {
		orientation?: Orientation;
		size?: number;
		minSize?: number;
		minSecondary?: number;
		collapsed?: boolean;
		disabled?: boolean;
		step?: number;
		a: Snippet;
		b: Snippet;
		onresize?: (size: number) => void;
		oncollapse?: (collapsed: boolean) => void;
	}

	let {
		orientation = 'horizontal',
		size = $bindable(240),
		minSize = 80,
		minSecondary = 80,
		collapsed = false,
		disabled = false,
		step = 16,
		a,
		b,
		onresize,
		oncollapse
	}: Props = $props();

	let container: HTMLDivElement | null = $state(null);
	let dragging = $state(false);
	let startPointer = 0;
	let startSize = 0;

	// horizontal split => children side by side => resize along X.
	const isHorizontal = $derived(orientation === 'horizontal');

	function params(): ResizeParams {
		const total = container
			? isHorizontal
				? container.clientWidth
				: container.clientHeight
			: size + minSecondary;
		return { total, minA: minSize, minB: minSecondary };
	}

	function onPointerDown(e: PointerEvent) {
		if (disabled || collapsed) return;
		dragging = true;
		startPointer = isHorizontal ? e.clientX : e.clientY;
		startSize = size;
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		e.preventDefault();
	}

	function onPointerMove(e: PointerEvent) {
		if (!dragging) return;
		const delta = (isHorizontal ? e.clientX : e.clientY) - startPointer;
		size = applyDelta(startSize, delta, params());
		onresize?.(size);
	}

	function onPointerUp(e: PointerEvent) {
		if (!dragging) return;
		dragging = false;
		try {
			(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
		} catch {
			/* pointer already released */
		}
	}

	function onKeyDown(e: KeyboardEvent) {
		if (disabled || collapsed) return;
		const next = keyResize(size, e.key, step, params());
		if (next !== size) {
			size = next;
			onresize?.(size);
			e.preventDefault();
		}
	}

	function toggleCollapse() {
		oncollapse?.(!collapsed);
	}

	const p = $derived(params());
</script>

<div
	bind:this={container}
	class="wt-split"
	class:vertical={!isHorizontal}
	class:collapsed
>
	<div
		class="wt-split-a"
		style={collapsed
			? 'flex:0 0 0'
			: isHorizontal
				? `flex:0 0 ${size}px`
				: `flex:0 0 ${size}px`}
	>
		{@render a()}
	</div>

	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div
		class="wt-sash"
		class:dragging
		role="separator"
		tabindex={disabled ? -1 : 0}
		aria-orientation={isHorizontal ? 'vertical' : 'horizontal'}
		aria-valuenow={Math.round(size)}
		aria-valuemin={p.minA}
		aria-valuemax={Math.round(p.total - p.minB)}
		aria-disabled={disabled}
		onpointerdown={onPointerDown}
		onpointermove={onPointerMove}
		onpointerup={onPointerUp}
		onkeydown={onKeyDown}
		ondblclick={toggleCollapse}
	></div>

	<div class="wt-split-b">
		{@render b()}
	</div>
</div>

<style>
	.wt-split {
		display: flex;
		flex-direction: row;
		width: 100%;
		height: 100%;
		min-height: 0;
		min-width: 0;
	}
	.wt-split.vertical {
		flex-direction: column;
	}
	.wt-split-a {
		overflow: auto;
		min-width: 0;
		min-height: 0;
	}
	.wt-split-b {
		flex: 1 1 auto;
		overflow: auto;
		min-width: 0;
		min-height: 0;
	}
	.wt-sash {
		flex: 0 0 6px;
		background: var(--color-border);
		cursor: col-resize;
		transition: background 0.12s ease;
		touch-action: none;
	}
	.wt-split.vertical .wt-sash {
		cursor: row-resize;
	}
	.wt-sash:hover,
	.wt-sash:focus-visible,
	.wt-sash.dragging {
		background: var(--color-accent, var(--color-primary, #7c8cff));
		outline: none;
	}
	.wt-split.collapsed .wt-sash {
		cursor: default;
	}
</style>
