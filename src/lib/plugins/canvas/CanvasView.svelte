<!--
  Canvas View — the main page for the AI Canvas plugin.

  Shows the currently active canvas (rendered by CanvasRenderer) or
  a creation prompt if no canvas is active.

  Closes the responsive-engine loop the same way the /canvas route does
  (src/routes/canvas/+page.svelte): attaches the viewport bridge (ui:viewport)
  and mirrors the app theme fact into ui:theme, so reflow + theme reaction work
  whether the canvas is reached via the route or this plugin pane.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { PluginContentArea, Button } from '@plures/design-dojo';
  import CanvasRenderer from '@plures/canvas-runtime/renderer';
  import { createReactiveGraph, toolCanvasCreate, getDemoCanvas } from '@plures/canvas-runtime';
  import { getSharedGraph } from '$lib/stores/plures-db-adapter.js';
  import { query } from '$lib/stores/praxis-svelte.svelte.js';
  import { mountViewportBridge, bridgeThemeToGraph } from '$lib/canvas/canvas-ui-bridge.js';
  import type { CanvasDocument } from '@plures/canvas-runtime';

  // Reactive graph wrapping the shared PluresDB graph
  const graph = createReactiveGraph(getSharedGraph());

  // Active canvas — loaded from PluresDB reactively
  // Using $state here is allowed — this is a local cache of PluresDB state.
  // The source of truth is graph.get('canvas:_active'), and we subscribe to it.
  // eslint-disable-next-line plures/no-raw-stores
  let activeCanvas: CanvasDocument | null = $state(
    graph.get('canvas:_active') as CanvasDocument | null
  );

  // Subscribe to active canvas changes (e.g. AI loads a new canvas)
  graph.subscribe('canvas:_active', (value) => {
    activeCanvas = value as CanvasDocument | null;
  });

  // ── Close the responsive-engine loop (same as the /canvas route) ──────────
  // 1) ui:viewport ← window resize (the IO edge). Browser-only; cleaned up on destroy.
  onMount(() => mountViewportBridge(graph));

  // 2) ui:theme ← the existing app theme fact ('theme.applied' = { value }).
  //    query() inside $effect tracks the rune, so this re-runs on every toggle;
  //    seeds once on mount from the current value.
  $effect(() => {
    bridgeThemeToGraph(graph, query('theme.applied'));
  });

  function seedCanvasData(canvas: CanvasDocument): void {
    for (const [key, value] of Object.entries(canvas.data)) {
      graph.put(`canvas:${key}`, value);
    }
  }

  function createNewCanvas() {
    const canvas = toolCanvasCreate({
      title: 'New Canvas',
      description: 'Created interactively',
      author: 'user',
    });
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

  function dbGet(key: string): unknown {
    return graph.get(key);
  }

  function dbSet(key: string, value: unknown): void {
    graph.put(key, value);
  }

  function dbSubscribe(key: string, callback: (value: unknown) => void): () => void {
    return graph.subscribe(key, callback);
  }
</script>

<PluginContentArea>
  {#if activeCanvas}
    <CanvasRenderer
      document={activeCanvas}
      {dbGet}
      {dbSet}
      {dbSubscribe}
      prefix="canvas:"
    />
  {:else}
    <Button variant="primary" onclick={createNewCanvas}>Create New Canvas</Button>
    <Button variant="secondary" onclick={createDemoCanvas}>Create Demo Canvas</Button>
  {/if}
</PluginContentArea>
