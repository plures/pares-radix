<script>
  const invoke = window.__TAURI__?.core?.invoke ?? (async () => ({}));

  let tree = $state(null);
  let loading = $state(true);
  let selected = $state(null);

  $effect(() => { loadConfig(); });

  async function loadConfig() {
    loading = true;
    try {
      const result = await invoke('config_tree', {});
      tree = result;
    } catch {
      tree = {
        name: 'datacenter',
        children: [
          { name: 'cluster-01', children: [
            { name: 'node-a.yaml', content: 'role: compute\ncpu: 64\nmemory: 256GB' },
            { name: 'node-b.yaml', content: 'role: storage\ncpu: 32\nmemory: 128GB' },
          ]},
          { name: 'cluster-02', children: [
            { name: 'node-c.yaml', content: 'role: compute\ncpu: 128\nmemory: 512GB' },
          ]},
          { name: 'global.yaml', content: 'region: westus2\nenv: production' },
        ],
      };
    }
    loading = false;
  }

  function selectNode(node) {
    selected = node;
  }
</script>

<div class="config-browser">
  <header class="cb-header">
    <h2>Config Browser</h2>
    <button class="refresh-btn" onclick={loadConfig}>↻ Reload</button>
  </header>

  <div class="cb-content">
    <div class="tree-panel">
      {#if loading}
        <p class="loading">Loading...</p>
      {:else if tree}
        {@render treeNode(tree, 0)}
      {/if}
    </div>

    {#if selected?.content}
      <div class="detail-panel">
        <h3>{selected.name}</h3>
        <pre>{selected.content}</pre>
      </div>
    {/if}
  </div>
</div>

{#snippet treeNode(node, depth)}
  <div class="tree-item" style="padding-left: {depth * 16}px">
    {#if node.children}
      <span class="folder" onclick={() => selectNode(node)}>📁 {node.name}</span>
      {#each node.children as child}
        {@render treeNode(child, depth + 1)}
      {/each}
    {:else}
      <span class="file" onclick={() => selectNode(node)}>📄 {node.name}</span>
    {/if}
  </div>
{/snippet}

<style>
  .config-browser { display: flex; flex-direction: column; height: 100%; overflow: hidden; padding: 16px; }
  .cb-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
  .cb-header h2 { font-size: 14px; font-weight: 600; color: var(--text-primary, #e8eaf0); margin: 0; }
  .refresh-btn { background: var(--bg-elevated, #1e2128); border: 1px solid var(--border, #2c2f38); border-radius: 4px; color: var(--text-secondary, #8b90a0); padding: 4px 8px; font-size: 12px; cursor: pointer; }
  .cb-content { display: flex; flex: 1; gap: 12px; overflow: hidden; }
  .tree-panel { flex: 1; overflow-y: auto; }
  .detail-panel { flex: 1; background: var(--bg-elevated, #1e2128); border-radius: 6px; padding: 12px; overflow-y: auto; }
  .detail-panel h3 { font-size: 13px; color: var(--text-primary, #e8eaf0); margin: 0 0 8px; }
  .detail-panel pre { font-size: 12px; color: var(--text-secondary, #8b90a0); font-family: var(--font-mono, monospace); white-space: pre-wrap; margin: 0; }
  .tree-item { cursor: pointer; padding: 2px 0; }
  .folder, .file { font-size: 12px; color: var(--text-secondary, #8b90a0); }
  .folder:hover, .file:hover { color: var(--text-primary, #e8eaf0); }
  .loading { color: var(--text-muted, #555a6a); font-size: 13px; }
</style>
