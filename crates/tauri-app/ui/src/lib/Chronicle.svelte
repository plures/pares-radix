<script>
  const invoke = window.__TAURI__?.core?.invoke ?? (async () => []);
  
  let entries = $state([]);
  let loading = $state(true);

  $effect(() => { loadEntries(); });

  async function loadEntries() {
    loading = true;
    try {
      const result = await invoke('chronos_recent', { limit: 50 });
      entries = result || [];
    } catch {
      entries = [
        { id: '1', timestamp: new Date().toISOString(), action: 'MessageReceived', key: 'tui:user', data: { content: 'hello' } },
        { id: '2', timestamp: new Date().toISOString(), action: 'ModelCalled', key: 'copilot:claude-sonnet-4.5', data: { latency_ms: 2100 } },
        { id: '3', timestamp: new Date().toISOString(), action: 'ResponseGenerated', key: 'tui:agent', data: { length: 42 } },
      ];
    }
    loading = false;
  }
</script>

<div class="chronicle">
  <header class="chronicle-header">
    <h2>Chronos Timeline</h2>
    <button class="refresh-btn" onclick={loadEntries}>↻ Refresh</button>
  </header>
  
  {#if loading}
    <p class="loading">Loading timeline...</p>
  {:else if entries.length === 0}
    <p class="empty">No events recorded yet. Use the chat to generate activity.</p>
  {:else}
    <div class="timeline">
      {#each entries as entry}
        <div class="entry">
          <span class="time">{new Date(entry.timestamp).toLocaleTimeString()}</span>
          <span class="action">{entry.action}</span>
          <span class="key">{entry.key}</span>
          {#if entry.data}
            <span class="data">{JSON.stringify(entry.data).slice(0, 80)}</span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .chronicle { display: flex; flex-direction: column; height: 100%; overflow: hidden; padding: 16px; }
  .chronicle-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
  .chronicle-header h2 { font-size: 14px; font-weight: 600; color: var(--text-primary, #e8eaf0); margin: 0; }
  .refresh-btn { background: var(--bg-elevated, #1e2128); border: 1px solid var(--border, #2c2f38); border-radius: 4px; color: var(--text-secondary, #8b90a0); padding: 4px 8px; font-size: 12px; cursor: pointer; }
  .refresh-btn:hover { background: var(--bg-hover, #262930); color: var(--text-primary, #e8eaf0); }
  .loading, .empty { color: var(--text-muted, #555a6a); font-size: 13px; text-align: center; padding: 32px 0; }
  .timeline { overflow-y: auto; flex: 1; display: flex; flex-direction: column; gap: 2px; }
  .entry { display: grid; grid-template-columns: 80px 140px 1fr 1fr; gap: 8px; padding: 6px 8px; border-radius: 4px; font-size: 12px; align-items: center; }
  .entry:hover { background: var(--bg-hover, #262930); }
  .time { color: var(--text-muted, #555a6a); font-family: var(--font-mono, monospace); font-size: 11px; }
  .action { color: var(--accent, #7c6af7); font-weight: 500; }
  .key { color: var(--text-secondary, #8b90a0); font-family: var(--font-mono, monospace); font-size: 11px; overflow: hidden; text-overflow: ellipsis; }
  .data { color: var(--text-muted, #555a6a); font-family: var(--font-mono, monospace); font-size: 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
