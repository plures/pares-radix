<script>
  import { ChronicleViewer } from '@plures/design-dojo';
  import { getChronosEntries } from './api.js';

  /** @type {import('@plures/design-dojo/dist/app/ChronicleViewer.svelte').ChronicleNode[]} */
  let nodes = $state([]);
  let loading = $state(true);

  $effect(() => { loadEntries(); });

  async function loadEntries() {
    loading = true;
    try {
      const result = await getChronosEntries(50);
      nodes = (result || []).map(e => ({
        id: e.id,
        timestamp: new Date(e.timestamp).getTime(),
        path: e.key || '',
        diff: { before: null, after: e.data },
        cause: e.action || null,
        context: null,
      }));
    } catch {
      nodes = [
        { id: '1', timestamp: Date.now(), path: 'tui:user', diff: { before: null, after: { content: 'hello' } }, cause: 'MessageReceived', context: null },
        { id: '2', timestamp: Date.now(), path: 'copilot:claude-sonnet-4.5', diff: { before: null, after: { latency_ms: 2100 } }, cause: 'ModelCalled', context: null },
        { id: '3', timestamp: Date.now(), path: 'tui:agent', diff: { before: null, after: { length: 42 } }, cause: 'ResponseGenerated', context: null },
      ];
    }
    loading = false;
  }

  function handleNodeSelect(node) {
    console.log('Chronicle node selected:', node);
  }

  function handleSearch(query) {
    console.log('Chronicle search:', query);
  }
</script>

<ChronicleViewer
  {nodes}
  onnodeselect={handleNodeSelect}
  onsearch={handleSearch}
  searching={loading}
/>
