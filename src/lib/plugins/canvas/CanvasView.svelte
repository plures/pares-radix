<!--
  Canvas View — the main page for the AI Canvas plugin.

  Shows the currently active canvas (rendered by CanvasRenderer) or
  a creation prompt if no canvas is active.
-->
<script lang="ts">
  import { PluginContentArea, Button } from '@plures/design-dojo';
  import CanvasRenderer from '@plures/canvas-runtime/renderer';
  import { createReactiveGraph, toolCanvasCreate } from '@plures/canvas-runtime';
  import { getSharedGraph } from '$lib/stores/plures-db-adapter.js';
  import type { CanvasDocument } from '@plures/canvas-runtime';

  // Reactive graph wrapping the shared PluresDB graph
  const graph = createReactiveGraph(getSharedGraph());

  // Active canvas — loaded from PluresDB or null
  let activeCanvas: CanvasDocument | null = $state(
    graph.get('canvas:_active') as CanvasDocument | null
  );

  function createNewCanvas() {
    const canvas = toolCanvasCreate({
      title: 'New Canvas',
      description: 'Created interactively',
      author: 'user',
    });
    activeCanvas = canvas;
    graph.put('canvas:_active', canvas);

    // Seed initial data
    for (const [key, value] of Object.entries(canvas.data)) {
      graph.put(`canvas:${key}`, value);
    }
  }

  function dbGet(key: string): unknown {
    return graph.get(key);
  }

  function dbSet(key: string, value: unknown): void {
    graph.put(key, value);
  }
</script>

<PluginContentArea>
  {#if activeCanvas}
    <CanvasRenderer
      document={activeCanvas}
      {dbGet}
      {dbSet}
      prefix="canvas:"
    />
  {:else}
    <Button variant="primary" onclick={createNewCanvas}>Create New Canvas</Button>
  {/if}
</PluginContentArea>
