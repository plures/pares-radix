<script>
  import '@plures/design-dojo/tokens.css';
  import { onMount } from 'svelte';
  import { initBuiltinPlugins } from './lib/plugins/index.js';
  import { activePlugins, pluginRegistry, allStatusBarItems } from './lib/plugins/registry.js';
  import { activeView, canvasPanes, focusedPane, commandPaletteOpen } from './lib/store.js';
  import { recordChronos } from './lib/api.js';
  import CommandPalette from './lib/CommandPalette.svelte';

  onMount(() => { initBuiltinPlugins(); });

  function handleActivityClick(pluginId, event) {
    if (event.ctrlKey || event.metaKey) {
      const newId = String(Date.now());
      $canvasPanes = [...$canvasPanes, { id: newId, pluginId }];
      $focusedPane = newId;
    } else {
      $canvasPanes = $canvasPanes.map(p =>
        p.id === $focusedPane ? { ...p, pluginId } : p
      );
    }
    recordChronos('Update', 'canvas', { action: event.ctrlKey || event.metaKey ? 'split' : 'switch', pluginId });
  }

  function closePane(paneId) {
    if ($canvasPanes.length <= 1) return;
    $canvasPanes = $canvasPanes.filter(p => p.id !== paneId);
    if ($focusedPane === paneId) {
      $focusedPane = $canvasPanes[0].id;
    }
    recordChronos('Update', 'canvas', { action: 'closePane', paneId });
  }

  function handleKeydown(e) {
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'P') {
      e.preventDefault();
      $commandPaletteOpen = !$commandPaletteOpen;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="radix-host">
  <!-- Activity Bar -->
  <nav class="radix-activity-bar">
    {#each $activePlugins.filter(p => p.view || p.component) as plugin}
      <button
        class="radix-activity-item"
        class:active={$activeView === plugin.id}
        onclick={(e) => handleActivityClick(plugin.id, e)}
        title={plugin.name}
      >
        <svg viewBox="0 0 16 16" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d={plugin.iconPath || ''} />
        </svg>
      </button>
    {/each}
  </nav>

  <!-- Canvas -->
  <main class="radix-canvas">
    {#each $canvasPanes as pane (pane.id)}
      {@const plugin = $activePlugins.find(p => p.id === pane.pluginId)}
      <section
        class="radix-pane"
        class:focused={$focusedPane === pane.id}
        onclick={() => $focusedPane = pane.id}
      >
        {#if plugin?.component}
          {@const Comp = plugin.component}
          <Comp />
        {:else}
          <div class="radix-welcome">
            <p class="radix-welcome-title">pares-radix</p>
            <p class="radix-welcome-hint">Select a plugin or press <kbd>Ctrl+Shift+P</kbd></p>
          </div>
        {/if}
        {#if $canvasPanes.length > 1}
          <button class="pane-close" onclick={(e) => { e.stopPropagation(); closePane(pane.id); }}>✕</button>
        {/if}
      </section>
    {/each}
  </main>

  <!-- Status Bar -->
  <footer class="radix-status-bar">
    <span class="radix-status-item">radix v1.40</span>
    {#each $allStatusBarItems.filter(i => i.position !== 'right') as item}
      <button class="radix-status-item radix-status-btn" onclick={item.onclick}>{item.text}</button>
    {/each}
    <span class="radix-status-spacer"></span>
    {#each $allStatusBarItems.filter(i => i.position === 'right').sort((a, b) => (b.priority || 0) - (a.priority || 0)) as item}
      <button class="radix-status-item radix-status-btn" onclick={item.onclick}>{item.text}</button>
    {/each}
  </footer>
</div>

<CommandPalette />

<style>
  :global(body) {
    margin: 0;
    background: #1e1e2e;
    color: #ccd;
    font-family: 'Segoe UI', system-ui, sans-serif;
    font-size: 13px;
    overflow: hidden;
  }

  .radix-host {
    display: grid;
    grid-template-columns: 48px 1fr;
    grid-template-rows: 1fr 22px;
    height: 100vh;
  }

  .radix-activity-bar {
    grid-row: 1 / 3;
    background: #16161e;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding-top: 8px;
    gap: 2px;
  }

  .radix-activity-item {
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-left: 2px solid transparent;
    color: #666;
    cursor: pointer;
    border-radius: 0;
  }
  .radix-activity-item:hover { color: #aaa; }
  .radix-activity-item.active {
    color: #ccd;
    border-left-color: #569cd6;
  }

  .radix-canvas {
    display: flex;
    flex: 1;
    background: #1e1e2e;
    overflow: hidden;
  }

  .radix-pane {
    flex: 1;
    overflow: auto;
    position: relative;
    border-left: 1px solid var(--border-subtle, #2d2d3d);
  }
  .radix-pane:first-child { border-left: none; }
  .radix-pane.focused { border-left-color: #569cd6; }

  .pane-close {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 20px;
    height: 20px;
    background: transparent;
    border: none;
    color: #666;
    cursor: pointer;
    font-size: 12px;
    border-radius: 3px;
    display: none;
  }
  .radix-pane:hover .pane-close { display: block; }
  .pane-close:hover { background: #333; color: #ccc; }

  .radix-welcome {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
    opacity: 0.4;
  }
  .radix-welcome-title { font-size: 24px; font-weight: 300; }
  .radix-welcome-hint { font-size: 12px; }
  .radix-welcome-hint kbd {
    background: #333;
    padding: 2px 6px;
    border-radius: 3px;
    font-family: inherit;
  }

  .radix-status-bar {
    grid-column: 1 / -1;
    background: #16161e;
    display: flex;
    align-items: center;
    padding: 0 12px;
    font-size: 11px;
    color: #888;
  }
  .radix-status-item { padding: 0 8px; }
  .radix-status-spacer { flex: 1; }
  .radix-status-btn {
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .radix-status-btn:hover { color: #ccd; }
</style>
