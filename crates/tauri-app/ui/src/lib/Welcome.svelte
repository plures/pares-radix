<script>
  import { canvasPanes, focusedPane } from './store.js';
  import { recordChronos } from './api.js';

  function openPlugin(pluginId) {
    $canvasPanes = $canvasPanes.map(p =>
      p.id === $focusedPane ? { ...p, pluginId } : p
    );
    recordChronos('Update', 'canvas', { action: 'welcome-open', pluginId });
  }
</script>

<div class="welcome">
  <div class="welcome-center">
    <h1 class="welcome-title">pares-radix</h1>
    <p class="welcome-version">v1.40</p>

    <div class="welcome-actions">
      <h2>Start</h2>
      <button class="welcome-action" onclick={() => openPlugin('chat')}>
        <span class="action-key">Ctrl+1</span>
        <span class="action-label">Chat with AI</span>
      </button>
      <button class="welcome-action" onclick={() => openPlugin('procedures')}>
        <span class="action-key">Ctrl+2</span>
        <span class="action-label">Praxis Procedures</span>
      </button>
      <button class="welcome-action" onclick={() => openPlugin('chronicle')}>
        <span class="action-key">Ctrl+3</span>
        <span class="action-label">Chronos Timeline</span>
      </button>
    </div>

    <div class="welcome-shortcuts">
      <h2>Shortcuts</h2>
      <div class="shortcut-row">
        <kbd>Ctrl+Shift+P</kbd> <span>Command Palette</span>
      </div>
      <div class="shortcut-row">
        <kbd>Ctrl+Click</kbd> <span>Split View</span>
      </div>
      <div class="shortcut-row">
        <kbd>Ctrl+`</kbd> <span>Toggle Terminal</span>
      </div>
    </div>
  </div>
</div>

<style>
  .welcome {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    background: var(--bg-base, #1e1e2e);
  }
  .welcome-center {
    text-align: center;
    max-width: 400px;
  }
  .welcome-title {
    font-size: 28px;
    font-weight: 300;
    color: var(--text-primary, #ccd);
    margin-bottom: 4px;
  }
  .welcome-version {
    font-size: 12px;
    color: var(--text-muted, #555);
    margin-bottom: 32px;
  }
  .welcome-actions, .welcome-shortcuts {
    text-align: left;
    margin-top: 24px;
  }
  .welcome-actions h2, .welcome-shortcuts h2 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-muted, #555);
    margin-bottom: 8px;
  }
  .welcome-action {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    color: var(--text-secondary, #888);
    cursor: pointer;
    border-radius: 4px;
    font-size: 13px;
    text-align: left;
  }
  .welcome-action:hover {
    background: var(--bg-hover, #363646);
    color: var(--text-primary, #ccd);
  }
  .action-key {
    font-size: 11px;
    color: var(--text-muted, #555);
    min-width: 60px;
    font-family: var(--font-mono, monospace);
  }
  .shortcut-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 10px;
    font-size: 12px;
    color: var(--text-secondary, #888);
  }
  .shortcut-row kbd {
    background: var(--bg-elevated, #2d2d3d);
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 11px;
    font-family: var(--font-mono, monospace);
    color: var(--text-muted, #666);
    min-width: 80px;
    text-align: center;
  }
</style>
