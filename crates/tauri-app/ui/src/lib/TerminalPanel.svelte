<!--
  TerminalPanel — Bottom panel with Terminal/Chronos/Logs tabs.
  Uses design-dojo components for TUI compatibility.
-->
<script>
  import { Tabs, Box } from '@plures/design-dojo/layout';
  import { Input, Text } from '@plures/design-dojo/primitives';
  import { runCommand, recordChronos } from './api.js';

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
    recordChronos('ToolInvoked', 'terminal', { command: cmd });
    terminalOutput += `$ ${cmd}\n`;
    terminalInput = '';
    try {
      const result = await runCommand(cmd);
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

<Box class="terminal-panel">
  <Tabs {tabs} bind:activeTab ontabchange={(key) => activeTab = key}>
    {#snippet children({ activeTab: currentTab })}
      <Box class="panel-content">
        {#if currentTab === 'terminal'}
          <Text class="output" monospace>{terminalOutput}</Text>
          <Box class="input-row">
            <Text monospace class="prompt">$</Text>
            <Input
              bind:value={terminalInput}
              placeholder="Enter command..."
              onsubmit={executeCommand}
              onkeydown={handleKeydown}
              class="terminal-input"
            />
          </Box>
        {:else if currentTab === 'chronos'}
          <Text class="output" monospace>{chronosEntries.map(e => JSON.stringify(e)).join('\n') || 'No Chronos events yet. Interact with the agent to generate activity.'}</Text>
        {:else}
          <Text class="output" monospace>{logLines.join('\n') || 'Logs appear here when the agent processes messages.'}</Text>
        {/if}
      </Box>
    {/snippet}
  </Tabs>
</Box>

<style>
  :global(.terminal-panel) {
    display: flex;
    flex-direction: column;
    height: 100%;
    border-top: 1px solid var(--border, var(--border-default, #2c2f38));
    background: var(--bg-base, var(--surface-0, #0e0f11));
  }

  :global(.panel-content) {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-height: 0;
  }

  :global(.output) {
    flex: 1;
    overflow-y: auto;
    padding: 8px 12px;
    font-size: 12px;
    color: var(--text-secondary, var(--fg-muted, #8b90a0));
    white-space: pre-wrap;
    margin: 0;
  }

  :global(.input-row) {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    border-top: 1px solid var(--border, var(--border-default, #2c2f38));
    flex-shrink: 0;
  }

  :global(.prompt) {
    color: var(--accent, var(--accent-primary, #7c6af7));
    font-size: 12px;
    flex-shrink: 0;
  }

  :global(.terminal-panel .terminal-input) {
    flex: 1;
    background: transparent;
    border: none;
    font-family: var(--font-mono, monospace);
    font-size: 12px;
  }
</style>
