<script>
  import { onMount, onDestroy } from 'svelte';
  import { ChronicleViewer } from '@plures/design-dojo';
  import { getChronosEntries, getChronosLog } from './api.js';
  import { componentEvent } from './telemetry.js';

  /** @type {import('@plures/design-dojo/dist/app/ChronicleViewer.svelte').ChronicleNode[]} */
  let nodes = $state([]);
  let loading = $state(true);
  let refreshInterval;

  $effect(() => { loadEntries(); });

  onMount(() => {
    componentEvent('Chronicle', 'mount');
    // Auto-refresh every 5 seconds to capture new Chronos entries
    refreshInterval = setInterval(loadEntries, 5000);
  });

  onDestroy(() => {
    componentEvent('Chronicle', 'destroy');
    if (refreshInterval) clearInterval(refreshInterval);
  });

  async function loadEntries() {
    loading = true;
    try {
      // Merge Tauri backend entries with browser-mode in-memory log
      const backendEntries = await getChronosEntries(50);
      const memoryEntries = getChronosLog().slice(-50);

      // Combine and deduplicate
      const all = [
        ...(backendEntries || []).map(e => ({
          id: e.id,
          timestamp: new Date(e.timestamp).getTime(),
          path: e.key || '',
          diff: { before: null, after: e.data },
          cause: e.action || null,
          context: null,
        })),
        ...memoryEntries.map((e, i) => ({
          id: `mem-${i}`,
          timestamp: new Date(e.timestamp).getTime(),
          path: e.key || '',
          diff: { before: null, after: e.data },
          cause: e.action || null,
          context: null,
        })),
      ];

      // Sort by timestamp, newest first
      all.sort((a, b) => b.timestamp - a.timestamp);
      nodes = all.slice(0, 100);
    } catch {
      // Fallback to in-memory log only
      const memoryEntries = getChronosLog();
      nodes = memoryEntries.map((e, i) => ({
        id: `mem-${i}`,
        timestamp: new Date(e.timestamp).getTime(),
        path: e.key || '',
        diff: { before: null, after: e.data },
        cause: e.action || null,
        context: null,
      }));
    }
    loading = false;
  }

  function handleNodeSelect(node) {
    componentEvent('Chronicle', 'node-selected', { path: node?.path, cause: node?.cause });
  }

  function handleSearch(query) {
    componentEvent('Chronicle', 'search', { query });
  }
</script>

<ChronicleViewer
  {nodes}
  onnodeselect={handleNodeSelect}
  onsearch={handleSearch}
  searching={loading}
/>
