<script>
  import '@plures/design-dojo/tokens.css';
  import { StatusBar, StatusBarItem, TitleBar, Sidebar } from '@plures/design-dojo/layout';

  import { onMount } from 'svelte';
  import { initBuiltinPlugins } from './lib/plugins/index.js';
  import { activePlugins } from './lib/plugins/registry.js';
  import PluginManager from './lib/PluginManager.svelte';
  import MemorySidebar from './lib/MemorySidebar.svelte';
  import Wizard from './lib/Wizard.svelte';
  import CommandPalette from './lib/CommandPalette.svelte';
  import { activeView, sidebarOpen, commandPaletteOpen, panelOpen, panelHeight } from './lib/store.js';
  import TerminalPanel from './lib/TerminalPanel.svelte';

  onMount(() => { initBuiltinPlugins(); });

  const tauriCore = typeof window !== 'undefined' ? window.__TAURI__?.core : undefined;
  const tauriEvent = typeof window !== 'undefined' ? window.__TAURI__?.event : undefined;
  const invoke = tauriCore?.invoke;
  const listen = tauriEvent?.listen;

  let agentName = $state('Pares Agens');

  /** @type {{ id: string, title: string, body: string, actions: { id: string, label: string }[] }[]} */
  let actionableNotifications = $state([]);

  // Activities are now derived from active plugins + extensions button

  function handleWizardComplete(/** @type {string} */ name) {
    agentName = name;
  }

  function dismissNotification(id) {
    actionableNotifications = actionableNotifications.filter((n) => n.id !== id);
  }

  async function triggerNotificationAction(notificationId, action) {
    if (invoke) {
      try {
        await invoke('handle_notification_action', { notificationId, action });
      } catch (err) {
        console.warn('Failed to handle notification action:', err);
      }
    }
    dismissNotification(notificationId);
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
    if (!listen) return;
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

  function handleMinimize() {
    if (typeof window !== 'undefined' && window.__TAURI__) {
      import('@tauri-apps/api/window').then(m => m.getCurrentWindow().minimize());
    }
  }
  function handleMaximize() {
    if (typeof window !== 'undefined' && window.__TAURI__) {
      import('@tauri-apps/api/window').then(m => m.getCurrentWindow().toggleMaximize());
    }
  }
  function handleClose() {
    if (typeof window !== 'undefined' && window.__TAURI__) {
      import('@tauri-apps/api/window').then(m => m.getCurrentWindow().close());
    }
  }
</script>

{#if actionableNotifications.length > 0}
  <section class="actionable-notifications" aria-live="polite" aria-label="Actionable notifications">
    {#each actionableNotifications as notification (notification.id)}
      <article class="actionable-notification-card">
        <h3>{notification.title}</h3>
        <p>{notification.body}</p>
        <div class="actionable-notification-actions">
          {#each notification.actions as action (action.id)}
            <button
              type="button"
              class={`actionable-btn ${action.id === 'view' ? 'secondary' : 'primary'}`}
              onclick={() => triggerNotificationAction(notification.id, action.id)}>
              {action.label}
            </button>
          {/each}
          <button type="button" class="actionable-btn secondary" onclick={() => dismissNotification(notification.id)}>
            Dismiss
          </button>
        </div>
      </article>
    {/each}
  </section>
{/if}

<svelte:window onkeydown={handleGlobalKeydown} />
<CommandPalette />
<Wizard onComplete={handleWizardComplete} />

<div class="shell">
  <TitleBar title="pares-radix" onminimize={handleMinimize} onmaximize={handleMaximize} onclose={handleClose} />

  <div class="workspace">
    <nav class="activity-bar">
      {#each $activePlugins as plugin (plugin.id)}
        <button
          class="activity-btn"
          class:active={$activeView === plugin.id}
          onclick={() => $activeView = plugin.id}
          title={plugin.name}
        >
          {plugin.icon}
        </button>
      {/each}
      <button
        class="activity-btn"
        class:active={$activeView === 'extensions'}
        onclick={() => $activeView = 'extensions'}
        title="Extensions"
      >
        🧩
      </button>
    </nav>

    <main class="editor-area">
      <div class="editor-content">
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
      </div>
      {#if $panelOpen}
        <div class="bottom-panel" style:height="{$panelHeight}px">
          <TerminalPanel />
        </div>
      {/if}
    </main>

    {#if $sidebarOpen}
      <Sidebar>
        <MemorySidebar />
      </Sidebar>
    {/if}
  </div>

  <StatusBar>
    <StatusBarItem>pares-radix</StatusBarItem>
    <StatusBarItem>PluresDB: connected</StatusBarItem>
    <StatusBarItem>
      <button class="panel-toggle-btn" onclick={() => $panelOpen = !$panelOpen} title="Toggle Terminal (Ctrl+`)">
        {$panelOpen ? '▼' : '▲'} Terminal
      </button>
    </StatusBarItem>
  </StatusBar>
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  .workspace {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .editor-area {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .editor-content {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .bottom-panel {
    flex-shrink: 0;
    overflow: hidden;
  }

  .panel-toggle-btn {
    background: transparent;
    border: none;
    color: inherit;
    cursor: pointer;
    font-size: 11px;
    padding: 0 4px;
  }

  .panel-toggle-btn:hover {
    color: var(--accent);
  }

  .activity-bar {
    width: 48px;
    background: var(--bg-surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 8px 0;
    gap: 4px;
    flex-shrink: 0;
  }

  .activity-btn {
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    font-size: 18px;
    cursor: pointer;
    transition: background var(--transition);
    position: relative;
  }

  .activity-btn:hover {
    background: var(--bg-hover);
  }

  .activity-btn.active {
    background: var(--bg-elevated);
  }

  .activity-btn.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 8px;
    bottom: 8px;
    width: 2px;
    background: var(--accent);
    border-radius: 1px;
  }
</style>
