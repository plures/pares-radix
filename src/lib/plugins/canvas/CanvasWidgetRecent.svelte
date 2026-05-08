<!--
  Recent Canvases Widget — dashboard widget showing saved canvases.
-->
<script lang="ts">
  import { Button } from '@plures/design-dojo';
  import { getSharedGraph } from '$lib/stores/plures-db-adapter.js';

  const graph = getSharedGraph();
  const canvasKeys = graph.keys('canvas:_saved:');
  const recentCanvases = canvasKeys.slice(0, 5).map((key) => {
    const data = graph.get(key) as { title?: string; modifiedAt?: string } | undefined;
    return {
      key,
      title: data?.title ?? 'Untitled',
      modifiedAt: data?.modifiedAt ?? '',
    };
  });
</script>

{#if recentCanvases.length > 0}
  {#each recentCanvases as canvas}
    <Button variant="secondary" onclick={() => {}}>{canvas.title}</Button>
  {/each}
{:else}
  <Button variant="primary" onclick={() => {}}>Create your first canvas</Button>
{/if}
