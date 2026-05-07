<script>
  import { commandPaletteOpen, activeView, sidebarOpen, panelOpen } from './store.js';
  import { allCommands } from './plugins/registry.js';

  let query = $state('');
  let selectedIndex = $state(0);

  // Built-in commands
  const builtinCommands = [
    { id: 'view.chat', label: 'View: Chat', action: () => { $activeView = 'chat'; close(); } },
    { id: 'view.procedures', label: 'View: Procedures', action: () => { $activeView = 'procedures'; close(); } },
    { id: 'view.settings', label: 'View: Settings', action: () => { $activeView = 'settings'; close(); } },
    { id: 'view.extensions', label: 'View: Extensions', action: () => { $activeView = 'extensions'; close(); } },
    { id: 'view.config', label: 'View: Config Browser', action: () => { $activeView = 'config-browser'; close(); } },
    { id: 'view.timeline', label: 'View: Timeline (Chronos)', action: () => { $activeView = 'chronicle'; close(); } },
    { id: 'sidebar.toggle', label: 'Toggle Sidebar', action: () => { $sidebarOpen = !$sidebarOpen; close(); } },
    { id: 'panel.toggle', label: 'Toggle Terminal Panel', action: () => { $panelOpen = !$panelOpen; close(); } },
    { id: 'theme.toggle', label: 'Toggle Theme', action: () => close() },
    { id: 'model.switch', label: 'Switch Model...', action: () => close() },
    { id: 'memory.search', label: 'Search Memory...', action: () => close() },
  ];

  // Merge built-in + plugin commands
  let commands = $derived([
    ...builtinCommands,
    ...$allCommands.map(cmd => ({ ...cmd, action: () => close() })),
  ]);

  $effect(() => {
    if ($commandPaletteOpen) {
      query = '';
      selectedIndex = 0;
    }
  });

  function close() { $commandPaletteOpen = false; }

  // Filter commands by query
  let filtered = $derived(
    query ? commands.filter(c => c.label.toLowerCase().includes(query.toLowerCase())) : commands
  );

  function handleKeydown(e) {
    if (e.key === 'ArrowDown') { selectedIndex = Math.min(selectedIndex + 1, filtered.length - 1); e.preventDefault(); }
    if (e.key === 'ArrowUp') { selectedIndex = Math.max(selectedIndex - 1, 0); e.preventDefault(); }
    if (e.key === 'Enter' && filtered[selectedIndex]) { filtered[selectedIndex].action(); e.preventDefault(); }
    if (e.key === 'Escape') { close(); e.preventDefault(); }
  }
</script>

{#if $commandPaletteOpen}
<div class="palette-overlay" onclick={close} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="palette" onclick={(e) => e.stopPropagation()}>
    <input
      class="palette-input"
      bind:value={query}
      placeholder="Type a command..."
      onkeydown={handleKeydown}
      autofocus
    />
    <div class="palette-results">
      {#each filtered as cmd, i}
        <button
          class="palette-item"
          class:selected={i === selectedIndex}
          onclick={cmd.action}
        >
          {cmd.label}
        </button>
      {/each}
    </div>
  </div>
</div>
{/if}

<style>
  .palette-overlay {
    position: fixed; inset: 0; z-index: 100;
    background: rgba(0,0,0,0.5);
    display: flex; justify-content: center; padding-top: 80px;
  }
  .palette {
    background: var(--bg-elevated, #1e2128);
    border: 1px solid var(--border, #2c2f38);
    border-radius: 8px;
    width: min(600px, 90vw);
    max-height: 400px;
    display: flex; flex-direction: column;
    box-shadow: 0 16px 48px rgba(0,0,0,0.4);
  }
  .palette-input {
    padding: 12px 16px;
    background: transparent;
    border: none; border-bottom: 1px solid var(--border, #2c2f38);
    color: var(--text-primary, #e8eaf0);
    font-size: 14px; outline: none;
    font-family: var(--font-sans, system-ui);
  }
  .palette-results { overflow-y: auto; padding: 4px; }
  .palette-item {
    display: block; width: 100%;
    padding: 8px 12px; text-align: left;
    background: transparent; border: none; border-radius: 4px;
    color: var(--text-secondary, #8b90a0);
    font-size: 13px; cursor: pointer;
  }
  .palette-item:hover, .palette-item.selected {
    background: var(--bg-hover, #262930);
    color: var(--text-primary, #e8eaf0);
  }
</style>
