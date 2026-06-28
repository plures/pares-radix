/**
 * UI Reactive Wiring — connects facts + authored tree → derived resolved tree.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * THE SPINE (C-PLURES-004), wired:
 *
 *   put ui:viewport / ui:theme / ui:density   (edge → bridge)
 *   put canvas:tree                            (author / AI)
 *                    │  subscribePrefix('ui:')  +  subscribe(authoredKey)
 *                    ▼
 *          resolveUiTree(authored, facts)       (PURE)
 *                    ▼
 *          put canvas:tree:resolved             (DERIVED — never authored)
 *                    ▼
 *          Unum reads :resolved → Svelte renders already-correct UI
 *
 * The authored tree is the pristine source of truth; the resolved tree is a
 * derived artifact, regenerated from clean source on every trigger. Re-resolution
 * never compounds because it always starts from the authored tree.
 * ─────────────────────────────────────────────────────────────────────────────
 */

import type { ReactiveGraph } from './reactive-graph.js';
import type { CanvasNodeLike } from './ui-facts.js';
import { resolveUiTree, type UiRuntimeFacts, type ViewportFact } from './ui-resolve.js';

export interface WireResolvedTreeOptions {
  /** Key holding the authored tree. Default 'canvas:tree'. */
  authoredKey?: string;
  /** Key to write the resolved tree to. Default 'canvas:tree:resolved'. */
  resolvedKey?: string;
  /** Prefix of trigger facts to react to. Default 'ui:'. */
  factPrefix?: string;
  /** Key of the viewport fact. Default 'ui:viewport'. */
  viewportKey?: string;
}

/** Read the current runtime facts off the graph. */
function readFacts(graph: ReactiveGraph, viewportKey: string): UiRuntimeFacts {
  const vp = graph.get(viewportKey) as ViewportFact | undefined;
  const facts: UiRuntimeFacts = {};
  if (vp && typeof vp.width === 'number') facts.viewport = vp;
  // theme/density reserved for follow-on practice sets.
  return facts;
}

/**
 * Wire reactive resolution. Whenever the authored tree OR any `ui:` fact changes,
 * re-resolve and write the derived tree. Returns a detach function.
 *
 * Idempotent writes: if there is no authored tree yet, nothing is written.
 */
export function wireResolvedTree(
  graph: ReactiveGraph,
  options: WireResolvedTreeOptions = {},
): () => void {
  const authoredKey = options.authoredKey ?? 'canvas:tree';
  const resolvedKey = options.resolvedKey ?? 'canvas:tree:resolved';
  const factPrefix = options.factPrefix ?? 'ui:';
  const viewportKey = options.viewportKey ?? 'ui:viewport';

  const reresolve = () => {
    const authored = graph.get(authoredKey) as CanvasNodeLike | undefined;
    if (!authored) return; // nothing to resolve yet
    const facts = readFacts(graph, viewportKey);
    const resolved = resolveUiTree(authored, facts);
    graph.put(resolvedKey, resolved);
  };

  // React to authored-tree changes.
  const unsubAuthored = graph.subscribe(authoredKey, () => reresolve());
  // React to any ui:* fact change (viewport/theme/density).
  const unsubFacts = graph.subscribePrefix(factPrefix, (key) => {
    // Avoid feedback loops: never react to our own resolved-tree writes (they're
    // under canvas:, not ui:, so this is just defensive).
    if (key === resolvedKey) return;
    reresolve();
  });

  // Seed once in case both keys already have values at wire time.
  reresolve();

  return () => {
    unsubAuthored();
    unsubFacts();
  };
}
