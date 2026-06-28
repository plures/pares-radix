<script lang="ts">
  import { onMount } from 'svelte';
  import { PluginContentArea, Button } from '@plures/design-dojo';
  import CanvasRenderer from '@plures/canvas-runtime/renderer';
  import { createReactiveGraph, toolCanvasCreate, getDemoCanvas } from '@plures/canvas-runtime';
  import { getSharedGraph } from '$lib/stores/plures-db-adapter.js';
  import { query } from '$lib/stores/praxis-svelte.svelte.js';
  import { mountViewportBridge, bridgeThemeToGraph } from '$lib/canvas/canvas-ui-bridge.js';
  import type { CanvasDocument } from '@plures/canvas-runtime';

  const graph = createReactiveGraph(getSharedGraph());

  // eslint-disable-next-line plures/no-raw-stores
  let activeCanvas: CanvasDocument | null = $state(null);

  // Subscribe to active canvas changes
  graph.subscribe('canvas:_active', (value) => {
    activeCanvas = value as CanvasDocument | null;
  });

  // ── Close the responsive-engine loop ──────────────────────────────────────
  // 1) ui:viewport ← window resize (the IO edge). Browser-only; cleaned up on destroy.
  onMount(() => mountViewportBridge(graph));

  // 2) ui:theme ← the existing app theme fact ('theme.applied' = { value }).
  //    Reading query() inside $effect tracks the rune, so this re-runs on toggle.
  //    Seeds once on mount from the current value, then mirrors every change.
  $effect(() => {
    bridgeThemeToGraph(graph, query('theme.applied'));
  });

  function seedCanvasData(canvas: CanvasDocument): void {
    for (const [key, value] of Object.entries(canvas.data)) {
      graph.put(`canvas:${key}`, value);
    }
  }

  function createNewCanvas() {
    const canvas = toolCanvasCreate({ title: 'New Canvas', author: 'user' });
    activeCanvas = canvas;
    graph.put('canvas:_active', canvas);
    seedCanvasData(canvas);
  }

  // Load the shared responsive demo canvas (same tree the behavior test verifies).
  // Resize the window narrow↔wide to see reflow; toggle the app theme to recolor text.
  function createDemoCanvas() {
    const canvas = getDemoCanvas();
    activeCanvas = canvas;
    graph.put('canvas:_active', canvas);
    seedCanvasData(canvas);
  }

  function dbGet(key: string): unknown { return graph.get(key); }
  function dbSet(key: string, value: unknown): void { graph.put(key, value); }
  function dbSubscribe(key: string, callback: (value: unknown) => void): () => void {
    return graph.subscribe(key, callback);
  }
</script>

<svelte:head><title>Canvas — Radix</title></svelte:head>

<PluginContentArea>
  {#if activeCanvas}
    <CanvasRenderer document={activeCanvas} {dbGet} {dbSet} {dbSubscribe} prefix="canvas:" />
  {:else}
    <Button variant="primary" onclick={createNewCanvas}>Create New Canvas</Button>
    <Button variant="secondary" onclick={createDemoCanvas}>Create Demo Canvas</Button>
  {/if}
</PluginContentArea>
