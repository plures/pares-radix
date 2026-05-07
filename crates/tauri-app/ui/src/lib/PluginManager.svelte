<script>
  import { pluginRegistry, togglePlugin } from './plugins/registry.js';
</script>

<div class="plugin-manager">
  <h2>Extensions</h2>
  {#each $pluginRegistry as plugin (plugin.id)}
    <div class="plugin-card">
      <span class="plugin-icon">{plugin.icon}</span>
      <div class="plugin-info">
        <span class="plugin-name">{plugin.name}</span>
        <span class="plugin-desc">{plugin.description}</span>
      </div>
      <button
        class="toggle-btn"
        class:enabled={plugin.enabled}
        onclick={() => togglePlugin(plugin.id)}
      >
        {plugin.enabled ? 'Disable' : 'Enable'}
      </button>
    </div>
  {/each}
</div>

<style>
  .plugin-manager {
    padding: 16px;
    overflow-y: auto;
    height: 100%;
  }

  .plugin-manager h2 {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary, #e8eaf0);
    margin: 0 0 16px;
  }

  .plugin-card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border-radius: var(--radius-sm, 6px);
    border: 1px solid var(--border, #2c2f38);
    background: var(--bg-surface, #1a1d24);
    margin-bottom: 8px;
  }

  .plugin-icon {
    font-size: 24px;
    flex-shrink: 0;
  }

  .plugin-info {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  .plugin-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary, #e8eaf0);
  }

  .plugin-desc {
    font-size: 12px;
    color: var(--text-secondary, #8b90a0);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .toggle-btn {
    padding: 4px 10px;
    font-size: 12px;
    border-radius: var(--radius-sm, 4px);
    border: 1px solid var(--border, #2c2f38);
    background: transparent;
    color: var(--text-secondary, #8b90a0);
    cursor: pointer;
    flex-shrink: 0;
  }

  .toggle-btn.enabled {
    border-color: var(--accent, #7c5cfc);
    color: var(--accent, #7c5cfc);
  }

  .toggle-btn:hover {
    background: var(--bg-hover, #262930);
  }
</style>
