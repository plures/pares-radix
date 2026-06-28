/**
 * CanvasRenderer responsive integration — renderer-side contract (Thread 2).
 *
 * The renderer (CanvasRenderer.svelte) was made responsive WITHOUT any caller
 * changes by:
 *   1. subscribing to the RAW `ui:viewport` key (bypassing the canvas: prefix),
 *   2. rendering `rendered = resolveUiTree(document.tree, { viewport })` instead
 *      of the authored `document.tree`, and
 *   3. extending isVisible(node) to hide a node whose RESOLVED props.hidden === true.
 *
 * A full DOM-mount test of the .svelte component is the ideal end-to-end check,
 * but this package's vitest runs in the default (node) environment with NO
 * component-render harness (no jsdom / @testing-library / svelte mount in tests/,
 * vitest.config.ts has no `environment`/`jsdom`). Rather than fabricate a heavy
 * DOM harness, we assert the renderer's DECISION LOGIC at the function level:
 * what the renderer feeds into `{@render renderNode(rendered)}` and into the new
 * `isVisible` branch. This is the exact, observable contract that integration
 * relies on.
 *
 * HONEST FOLLOW-ON (deferred): a DOM-mount test that actually mounts
 * CanvasRenderer.svelte, writes `ui:viewport` via the dbSubscribe prop, and
 * asserts that the rendered DOM (a) reflows direction and (b) omits a
 * hidden-at-breakpoint node. That requires adding a jsdom/svelte-testing harness
 * to this package (vitest `environment: 'jsdom'` + a mount helper), which is out
 * of scope for this thread and tracked as follow-on work.
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { registerComponent } from '../src/registry.js';
import { resolveUiTree } from '../src/ui-resolve.js';
import type { CanvasNodeLike } from '../src/ui-facts.js';

// Same stub-registration pattern as ui-resolve.test.ts: register stand-ins with
// the right CATEGORY so schemaKind inference works (Box → layout → container).
beforeAll(() => {
  const base = {
    component: null as unknown as never,
    props: [],
    hasChildren: true,
    description: 'stub',
  };
  registerComponent('Box', { ...base, name: 'Box', category: 'layout' });
  registerComponent('Button', { ...base, name: 'Button', category: 'input', hasChildren: false });
});

const box = (extra: Partial<CanvasNodeLike> = {}, children: CanvasNodeLike[] = []): CanvasNodeLike => ({
  id: 'b', type: 'Box', props: {}, children, ...extra,
});

/**
 * Mirror of the renderer's NEW isVisible `hidden` branch. The renderer also
 * evaluates `node.visible` conditions (unchanged), but the responsive contract
 * this thread adds is precisely: "a node whose RESOLVED props.hidden === true
 * does not render." We assert that branch against the resolver's real output.
 */
function hiddenByResolution(node: CanvasNodeLike): boolean {
  return node.props?.hidden === true;
}

describe('CanvasRenderer responsive contract — resolved props.hidden drives visibility', () => {
  it('responsive.hidden { base:true, lg:false } → hidden at 500, visible at 1300', () => {
    const tree = box({ responsive: { hidden: { base: true, lg: false } } });

    const narrow = resolveUiTree(tree, { viewport: { width: 500 } }); // base
    expect(narrow.props?.hidden).toBe(true);
    expect(hiddenByResolution(narrow)).toBe(true); // renderer would NOT render it

    const wide = resolveUiTree(tree, { viewport: { width: 1300 } }); // xl ≥ lg
    expect(wide.props?.hidden).toBe(false);
    expect(hiddenByResolution(wide)).toBe(false); // renderer WOULD render it
  });

  it('no viewport → identity clone → hidden stays unset → renderer renders (unchanged behavior)', () => {
    const tree = box({ responsive: { hidden: { base: true, lg: false } } });
    const out = resolveUiTree(tree, {}); // no viewport fact
    expect(out.props?.hidden).toBeUndefined();
    expect(hiddenByResolution(out)).toBe(false);
  });

  it('hidden on a nested child is honored independently of the parent', () => {
    const tree = box({}, [
      box({ id: 'child', responsive: { hidden: { base: false, lg: true } } }),
    ]);
    const wide = resolveUiTree(tree, { viewport: { width: 1300 } });
    expect(hiddenByResolution(wide)).toBe(false); // parent visible
    expect(hiddenByResolution(wide.children![0])).toBe(true); // child hidden at lg
  });
});

describe('CanvasRenderer responsive contract — direction collapses for the rendered tree', () => {
  it('responsive.direction { base:column, md:row } collapses by viewport width', () => {
    const tree = box({ responsive: { direction: { base: 'column', md: 'row' } } });
    expect(resolveUiTree(tree, { viewport: { width: 500 } }).props?.direction).toBe('column'); // base
    expect(resolveUiTree(tree, { viewport: { width: 900 } }).props?.direction).toBe('row'); // md
  });

  it('the renderer never mutates the authored document.tree (resolve clones)', () => {
    const tree = box({ responsive: { direction: { base: 'column', md: 'row' } } });
    const snapshot = JSON.parse(JSON.stringify(tree));
    const rendered = resolveUiTree(tree, { viewport: { width: 900 } });
    expect(tree).toEqual(snapshot); // authored intent pristine
    expect(rendered).not.toBe(tree); // rendered is a derived clone
    expect(rendered.props?.direction).toBe('row');
  });
});
