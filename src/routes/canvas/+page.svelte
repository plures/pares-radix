<script lang="ts">
  import { PluginContentArea, Button } from '@plures/design-dojo';
  import CanvasRenderer from '@plures/canvas-runtime/renderer';
  import { createReactiveGraph, toolCanvasCreate } from '@plures/canvas-runtime';
  import { getSharedGraph } from '$lib/stores/plures-db-adapter.js';
  import type { CanvasDocument } from '@plures/canvas-runtime';

  const graph = createReactiveGraph(getSharedGraph());

  // eslint-disable-next-line plures/no-raw-stores
  let activeCanvas: CanvasDocument | null = $state(null);

  // Subscribe to active canvas changes
  graph.subscribe('canvas:_active', (value) => {
    activeCanvas = value as CanvasDocument | null;
  });

  function createNewCanvas() {
    const canvas = toolCanvasCreate({ title: 'New Canvas', author: 'user' });
    activeCanvas = canvas;
    graph.put('canvas:_active', canvas);
    for (const [key, value] of Object.entries(canvas.data)) {
      graph.put(`canvas:${key}`, value);
    }
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
  {/if}
</PluginContentArea>
