<!--
  TerminalPanel — Bottom panel with Terminal/Chronos/Logs tabs.
  Uses design-dojo components for TUI compatibility.
-->
<script>
  import { Tabs } from '@plures/design-dojo/layout';
  import { Input } from '@plures/design-dojo/primitives';

  const invoke = window.__TAURI__?.core?.invoke ?? (async () => '');

  let activeTab = $state('terminal');
  let terminalOutput = $state('$ pares-radix ready\n');
  let terminalInput = $state('');
  let chronosEntries = $state([]);
  let logLines = $state([]);

  const tabs = [
    { key: 'terminal', label: 'Terminal', icon: '>' },
    { key: 'chronos', label: 'Chronos', icon: '⏱' },
    { key: 'logs', label: 'Logs', icon: '📋' },
  ];

  async function executeCommand() {
    if (!terminalInput.trim()) return;
    const cmd = terminalInput.trim();
    terminalOutput += `$ ${cmd}\n`;
    terminalInput = '';
    try {
      const result = await invoke('run_shell_command', { command: cmd });
      terminalOutput += result + '\n';
    } catch (e) {
      terminalOutput += `Error: ${e}\n`;
    }
  }

  function handleKeydown(e) {
    if (e.key === 'Enter') {
      executeCommand();
    }
  }
</script>

<div class="terminal-panel">
  <Tabs {tabs} bind:activeTab ontabchange={(key) => activeTab = key}>
    {#snippet children({ activeTab: currentTab })}
      <div class="panel-content">
        {#if currentTab === 'terminal'}
          <pre class="output">{terminalOutput}</pre>
          <div class="input-row">
            <span class="prompt">$</span>
            <Input
              bind:value={terminalInput}
              placeholder="Enter command..."
              onsubmit={executeCommand}
              onkeydown={handleKeydown}
              class="terminal-input"
            />
          </div>
        {:else if currentTab === 'chronos'}
          <pre class="output">{chronosEntries.map(e => JSON.stringify(e)).join('\n') || 'No Chronos events yet. Interact with the agent to generate activity.'}</pre>
        {:else}
          <pre class="output">{logLines.join('\n') || 'Logs appear here when the agent processes messages.'}</pre>
        {/if}
      </div>
    {/snippet}
  </Tabs>
</div>

<style>
  .terminal-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    border-top: 1px solid var(--border, var(--border-default, #2c2f38));
    background: var(--bg-base, var(--surface-0, #0e0f11));
  }

  .panel-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-height: 0;
  }

  .output {
    flex: 1;
    overflow-y: auto;
    padding: 8px 12px;
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    color: var(--text-secondary, var(--fg-muted, #8b90a0));
    white-space: pre-wrap;
    margin: 0;
  }

  .input-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    border-top: 1px solid var(--border, var(--border-default, #2c2f38));
    flex-shrink: 0;
  }

  .prompt {
    color: var(--accent, var(--accent-primary, #7c6af7));
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    flex-shrink: 0;
  }

  .terminal-panel :global(.terminal-input) {
    flex: 1;
    background: transparent;
    border: none;
    font-family: var(--font-mono, monospace);
    font-size: 12px;
  }
</style>
