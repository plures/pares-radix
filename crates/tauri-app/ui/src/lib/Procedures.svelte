<script>
  import { listProcedures, getProcedure, getProcedureLog, toggleProcedure, saveProcedure as saveProcedureApi, createFromTemplate as createFromTemplateApi } from '../api.js';
  import { Dialog } from '@plures/design-dojo/overlays';
  import { Box } from '@plures/design-dojo/layout';
  import { Button, Input, Text, Toggle, Select } from '@plures/design-dojo/primitives';
  import { List, ListItem, Table } from '@plures/design-dojo/data';
  import { Pane } from '@plures/design-dojo/surfaces';
  import { EmptyState } from '@plures/design-dojo';

  let { open = $bindable(false) } = $props();

  /**
   * @typedef {{ name: string, eventType: string, priority: number, enabled: boolean, body: string }} ProcRecord
   * @typedef {{ procedureName: string, firedAt: string, durationMs: number, triggerEvent: string }} LogEntry
   */

  /** @type {ProcRecord[]} */
  let procedures = $state([]);
  /** @type {ProcRecord | null} */
  let selected = $state(null);
  /** @type {LogEntry[]} */
  let logEntries = $state([]);
  let editMode = $state(false);
  let editBody = $state('');
  let templateValue = $state('');

  $effect(() => {
    if (open) {
      selected = null;
      editMode = false;
      editBody = '';
      logEntries = [];
      refreshProcedures();
    }
  });

  async function refreshProcedures() {
    try {
      procedures = await listProcedures();
    } catch {
      procedures = [];
    }
  }

  async function selectProcedure(name) {
    try {
      const rec = await getProcedure(name);
      if (!rec) return;
      selected = rec;
      editBody = rec.body;
      editMode = false;
      await refreshLog(name);
    } catch (err) {
      console.error('selectProcedure error:', err);
    }
  }

  async function refreshLog(name) {
    try {
      logEntries = await getProcedureLog(name, 50);
    } catch {
      logEntries = [];
    }
  }

  async function toggleEnabled(enabled) {
    if (!selected) return;
    try {
      await toggleProcedure(selected.name, enabled);
      selected = { ...selected, enabled };
      await refreshProcedures();
    } catch (err) {
      alert(`Failed to toggle procedure: ${err}`);
    }
  }

  async function saveProcedure() {
    if (!selected) return;
    try {
      const record = { ...selected, body: editBody };
      await saveProcedureApi(record);
      selected = record;
      editMode = false;
      await refreshProcedures();
    } catch (err) {
      alert(`Failed to save procedure: ${err}`);
    }
  }

  async function createFromTemplate() {
    if (!templateValue) return;
    try {
      const rec = await createFromTemplateApi(templateValue);
      templateValue = '';
      await refreshProcedures();
      await selectProcedure(rec.name);
    } catch (err) {
      alert(`Failed to create procedure: ${err}`);
    }
  }

  const templateOptions = [
    { value: '', label: '— New from template —' },
    { value: 'greeting', label: 'greeting' },
    { value: 'scheduled_task', label: 'scheduled task' },
    { value: 'approval_gate', label: 'approval gate' },
    { value: 'memory_pattern', label: 'memory pattern' },
  ];

  let logTableColumns = [
    { key: 'time', label: 'Time' },
    { key: 'duration', label: 'Duration' },
    { key: 'trigger', label: 'Trigger' },
  ];

  let logTableRows = $derived(logEntries.map(e => ({
    time: new Date(e.firedAt).toLocaleTimeString(),
    duration: `${e.durationMs} ms`,
    trigger: e.triggerEvent,
  })));
</script>

{#if open}
<Dialog onclose={() => open = false} title="⚡ Procedures">
  <Box border="none" class="procedures-body">
    <!-- Left: procedure list -->
    <Box border="none" class="proc-list-panel">
      <Box border="none" class="proc-list-toolbar">
        <Select
          bind:value={templateValue}
          options={templateOptions}
        />
        <Button variant="solid" size="sm" onclick={createFromTemplate}>＋</Button>
      </Box>
      <List>
        {#if procedures.length === 0}
          <ListItem>
            {#snippet children()}
              <Text>No procedures registered.</Text>
            {/snippet}
          </ListItem>
        {:else}
          {#each procedures as proc (proc.name)}
            <ListItem onclick={() => selectProcedure(proc.name)}>
              {#snippet children()}
                <Box border="none" class="proc-row">
                  <Text class="proc-dot {proc.enabled ? 'enabled' : 'disabled'}">●</Text>
                  <Text>{proc.name}</Text>
                  <Text class="proc-type">{proc.eventType}</Text>
                </Box>
              {/snippet}
            </ListItem>
          {/each}
        {/if}
      </List>
    </Box>

    <!-- Right: editor + log -->
    <Box border="none" class="proc-editor-panel">
      {#if !selected}
        <Text>Select a procedure to view or edit it.</Text>
      {:else}
        <Box border="none" class="proc-editor-toolbar">
          <Box border="none" class="proc-meta">
            <Text>{selected.name}</Text>
            <Text class="proc-badge">{selected.eventType}</Text>
          </Box>
          <Box border="none" class="proc-editor-actions">
            <Toggle checked={selected.enabled} onchange={toggleEnabled} />
            <Button variant="outline" size="sm"
              onclick={() => { editMode = !editMode; if (!editMode) editBody = selected.body; }}>
              {editMode ? 'Cancel' : 'Edit'}
            </Button>
            {#if editMode}
              <Button variant="solid" size="sm" onclick={saveProcedure}>Save</Button>
            {/if}
          </Box>
        </Box>

        <Input
          class="proc-body"
          value={editBody}
          oninput={(e) => editBody = e.target.value}
          disabled={!editMode}
        />

        <Box border="none" class="proc-log-section">
          <Text>Execution Log</Text>
          <Table columns={logTableColumns} rows={logTableRows} />
        </Box>
      {/if}
    </Box>
  </Box>
</Dialog>
{/if}

<style>
  :global(.procedures-body) {
    display: flex;
    gap: 16px;
    min-height: 400px;
  }

  :global(.proc-list-panel) {
    width: 240px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  :global(.proc-list-toolbar) {
    display: flex;
    gap: 8px;
  }

  :global(.proc-editor-panel) {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  :global(.proc-row) {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  :global(.proc-dot.enabled) { color: #34d399; }
  :global(.proc-dot.disabled) { color: #555; }

  :global(.proc-type) {
    font-size: 11px;
    color: var(--text-muted);
    margin-left: auto;
  }

  :global(.proc-editor-toolbar) {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  :global(.proc-meta) {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  :global(.proc-badge) {
    font-size: 11px;
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--bg-elevated);
  }

  :global(.proc-editor-actions) {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  :global(.proc-body) {
    flex: 1;
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    min-height: 200px;
  }

  :global(.proc-log-section) {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
</style>
