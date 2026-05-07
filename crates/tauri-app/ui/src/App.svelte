<script>
  import '@plures/design-dojo/tokens.css';
  import { StatusBar, StatusBarItem, TitleBar, Sidebar, Box, ActivityBar, Button, CommandPalette } from '@plures/design-dojo';

  import { onMount } from 'svelte';
  import { initBuiltinPlugins } from './lib/plugins/index.js';
  import { activePlugins } from './lib/plugins/registry.js';
  import PluginManager from './lib/PluginManager.svelte';
  import MemorySidebar from './lib/MemorySidebar.svelte';
  import Wizard from './lib/Wizard.svelte';
  import { activeView, sidebarOpen, commandPaletteOpen, panelOpen, panelHeight } from './lib/store.js';
  import { praxisViolationCount } from './lib/praxis.js';
  import TerminalPanel from './lib/TerminalPanel.svelte';
  import { listen, handleNotificationAction, minimizeWindow, maximizeWindow, closeWindow } from './api.js';

  onMount(() => { initBuiltinPlugins(); });

  let agentName = $state('Pares Agens');

  /** @type {{ id: string, title: string, body: string, actions: { id: string, label: string }[] }[]} */
  let actionableNotifications = $state([]);

  function handleWizardComplete(/** @type {string} */ name) {
    agentName = name;
  }

  function dismissNotification(id) {
    actionableNotifications = actionableNotifications.filter((n) => n.id !== id);
  }

  async function triggerNotificationAction(notificationId, action) {
    try {
      await handleNotificationAction({ notificationId, action });
    } catch (err) {
      console.warn('Failed to handle notification action:', err);
    }
    dismissNotification(notificationId);
  }

  // Activity bar items derived from active plugins
  let activityItems = $derived([
    ...$activePlugins.map(p => ({ key: p.id, label: p.name, icon: p.icon })),
    { key: 'extensions', label: 'Extensions', icon: '🧩' },
  ]);

  // Command palette commands
  let paletteCommands = $derived([
    { id: 'view.chat', label: 'View: Chat', category: 'View' },
    { id: 'view.procedures', label: 'View: Procedures', category: 'View' },
    { id: 'view.settings', label: 'View: Settings', category: 'View' },
    { id: 'view.extensions', label: 'View: Extensions', category: 'View' },
    { id: 'view.config', label: 'View: Config Browser', category: 'View' },
    { id: 'view.timeline', label: 'View: Timeline (Chronos)', category: 'View' },
    { id: 'sidebar.toggle', label: 'Toggle Sidebar', category: 'General', shortcut: 'Ctrl+B' },
    { id: 'panel.toggle', label: 'Toggle Terminal Panel', category: 'General', shortcut: 'Ctrl+`' },
    { id: 'theme.toggle', label: 'Toggle Theme', category: 'General' },
    { id: 'model.switch', label: 'Switch Model...', category: 'Model' },
    { id: 'memory.search', label: 'Search Memory...', category: 'Memory' },
  ]);

  function handleCommandSelect(id) {
    const actions = {
      'view.chat': () => $activeView = 'chat',
      'view.procedures': () => $activeView = 'procedures',
      'view.settings': () => $activeView = 'settings',
      'view.extensions': () => $activeView = 'extensions',
      'view.config': () => $activeView = 'config-browser',
      'view.timeline': () => $activeView = 'chronicle',
      'sidebar.toggle': () => $sidebarOpen = !$sidebarOpen,
      'panel.toggle': () => $panelOpen = !$panelOpen,
    };
    actions[id]?.();
    $commandPaletteOpen = false;
  }

  // Global keyboard shortcuts
  function handleGlobalKeydown(e) {
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'P') {
      e.preventDefault();
      $commandPaletteOpen = !$commandPaletteOpen;
    }
    if ((e.ctrlKey || e.metaKey) && e.key === '`') {
      e.preventDefault();
      $panelOpen = !$panelOpen;
    }
  }

  $effect(() => {
    const unlisten = listen('actionable-notification', (event) => {
      const payload = event.payload;
      if (!payload || !payload.id) return;
      actionableNotifications = [
        payload,
        ...actionableNotifications.filter((n) => n.id !== payload.id)
      ].slice(0, 3);
    });
    return () => { unlisten.then((fn) => fn?.()); };
  });

  function handleMinimize() { minimizeWindow(); }
  function handleMaximize() { maximizeWindow(); }
  function handleClose() { closeWindow(); }
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<CommandPalette
  commands={paletteCommands}
  bind:open={$commandPaletteOpen}
  onselect={handleCommandSelect}
  onclose={() => $commandPaletteOpen = false}
  placeholder="Type a command..."
/>

{#if false}<Wizard onComplete={handleWizardComplete} />{/if}

<Box border="none" class="shell" height="100vh">
  <TitleBar title="pares-radix" onminimize={handleMinimize} onmaximize={handleMaximize} onclose={handleClose} />

  <Box border="none" class="workspace">
    <ActivityBar
      items={activityItems}
      activeKey={$activeView}
      onselect={(key) => $activeView = key}
    />

    <Box border="none" class="editor-area">
      <Box border="none" class="editor-content">
        {#each $activePlugins as plugin (plugin.id)}
          {#if $activeView === plugin.id && plugin.component}
            {#if plugin.id === 'chat'}
              <plugin.component agentName={agentName} settingsOpen={false} proceduresOpen={false} />
            {:else}
              <plugin.component open={true} />
            {/if}
          {/if}
        {/each}
        {#if $activeView === 'extensions'}
          <PluginManager />
        {/if}
      </Box>
      {#if $panelOpen}
        <Box border="none" class="bottom-panel" height="{$panelHeight}px">
          <TerminalPanel />
        </Box>
      {/if}
    </Box>

    {#if $sidebarOpen}
      <Sidebar>
        <MemorySidebar />
      </Sidebar>
    {/if}
  </Box>

  <StatusBar>
    <StatusBarItem>pares-radix</StatusBarItem>
    <StatusBarItem>PluresDB: connected</StatusBarItem>
    {#if $praxisViolationCount > 0}
      <StatusBarItem>⚠️ {$praxisViolationCount} violation{$praxisViolationCount > 1 ? 's' : ''}</StatusBarItem>
    {/if}
    <StatusBarItem>
      <Button variant="ghost" size="sm" onclick={() => $panelOpen = !$panelOpen}>
        {$panelOpen ? '▼' : '▲'} Terminal
      </Button>
    </StatusBarItem>
  </StatusBar>
</Box>

<style>
  :global(.shell) {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  :global(.workspace) {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  :global(.editor-area) {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  :global(.editor-content) {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  :global(.bottom-panel) {
    flex-shrink: 0;
    overflow: hidden;
  }
</style>
