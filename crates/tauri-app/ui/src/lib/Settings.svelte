<script>
  import {
    getSettings, setSettings, listProviders, addProvider, updateProvider, removeProvider,
    listMcpTools, restartMcpServers, getLicenseStatus, activateLicense as activateLicenseApi,
    getTelemetrySnapshot, uploadTelemetrySnapshot as uploadTelemetrySnapshotApi
  } from '../api.js';
  import { Button, Input, Text, Toggle, Select } from '@plures/design-dojo/primitives';
  import { Box, Tabs } from '@plures/design-dojo/layout';
  import { Dialog } from '@plures/design-dojo/overlays';
  import { List, ListItem, Table } from '@plures/design-dojo/data';

  import MarketplaceTab from './MarketplaceTab.svelte';

  let { open = $bindable(false) } = $props();

  /** @type {HTMLDialogElement} */
  let dialog = $state(null);

  // ── Tab state ───────────────────────────────────────────────────────────
  /** @type {'providers'|'routing'|'channels'|'preferences'|'mcp'|'telemetry'|'license'|'marketplace'} */
  let activeTab = $state('providers');
  /** @type {HTMLButtonElement[]} */
  let tabButtons = $state([]);

  const TABS = /** @type {const} */ (['providers', 'routing', 'channels', 'preferences', 'mcp', 'telemetry', 'license', 'marketplace']);

  // ── Provider state ───────────────────────────────────────────────────────
  /**
   * @typedef {{ name: string, baseUrl: string, apiKey: string|null, models: string[] }} Provider
   */
  /** @type {Provider[]} */
  let providers = $state([]);
  let showProviderForm = $state(false);
  let providerFormName = $state('');
  let providerFormUrl = $state('');
  let providerFormKey = $state('');
  let providerFormModels = $state('');
  /** @type {string|null} editProviderName is set when editing an existing provider */
  let editProviderName = $state(null);

  // ── Ollama quick-config state ────────────────────────────────────────────
  /** Ollama server base URL (without /v1 — the HTTP client appends that). */
  let ollamaUrl   = $state('http://localhost:11434');
  /** Model name to use for interactive chat. */
  let ollamaModel = $state('llama3');

  // ── Routing state ────────────────────────────────────────────────────────
  /**
   * @typedef {{ provider: string, model: string }} ModelRef
   * @typedef {{ interactive?: ModelRef, background?: ModelRef, coding?: ModelRef }} RoutingPrefs
   */
  let routingInteractiveProvider = $state('');
  let routingInteractiveModel = $state('');
  let routingBackgroundProvider = $state('');
  let routingBackgroundModel = $state('');
  let routingCodingProvider = $state('');
  let routingCodingModel = $state('');

  // ── Channel state ────────────────────────────────────────────────────────
  /**
   * @typedef {{ kind: string, enabled: boolean, botToken?: string, phoneNumber?: string }} ChannelAdapter
   */
  /** @type {ChannelAdapter[]} */
  let channelAdapters = $state([]);

  // ── Preferences state ────────────────────────────────────────────────────
  let prefAgentName = $state('');
  let prefPersonalityNotes = $state('');
  let prefAutoRecall = $state(true);
  let prefCaptureCategories = $state(/** @type {string[]} */ ([]));
  let prefNotificationsEnabled = $state(true);
  let prefAutoStart = $state(false);
  let prefActivationHotkey = $state('Ctrl+Space');
  let prefSystemPrompt = $state('');

  const ALL_CAPTURE_CATEGORIES = ['code-pattern', 'preference', 'decision', 'error'];

  // ── MCP state ────────────────────────────────────────────────────────────
  /**
   * @typedef {{ name: string, command: string, args: string[], enabled: boolean }} McpServer
   * @typedef {{ serverName: string, name: string, description: string|null, inputSchema: any }} DiscoveredTool
   */
  /** @type {McpServer[]} */
  let mcpServers = $state([]);
  /** @type {DiscoveredTool[]} */
  let mcpTools = $state([]);
  let showMcpForm = $state(false);
  let mcpFormName = $state('');
  let mcpFormCommand = $state('');
  let mcpFormArgs = $state('');
  let mcpFormEnabled = $state(true);
  /** @type {string|null} */
  let editMcpName = $state(null);
  let mcpRestarting = $state(false);

  // ── License state ────────────────────────────────────────────────────────
  /**
   * @typedef {{ tier: 'free'|'pro', valid: boolean, expires_at?: string }} LicenseStatus
   */
  /** @type {LicenseStatus} */
  let licenseStatus = $state({ tier: 'free', valid: true });
  let licenseKey = $state('');
  let licenseError = $state('');
  let licenseActivating = $state(false);

  // ── Telemetry state ──────────────────────────────────────────────────────
  let telemetryEnabled = $state(false);
  let telemetryUploadEnabled = $state(false);
  let telemetryUploadEndpoint = $state('');
  /** @type {{ modelCallsByDay?: Record<string, number>, toolUsageFrequency?: Record<string, number>, avgLatencyMs?: number|null, latencySampleCount?: number, latencyMinMs?: number|null, latencyMaxMs?: number|null, lastUploadAt?: string|null }} */
  let telemetrySnapshot = $state({});
  let telemetryUploading = $state(false);
  let telemetryUploadError = $state('');

  // ── Dialog lifecycle ─────────────────────────────────────────────────────
  $effect(() => {
    if (!dialog) return;
    if (open) {
      activeTab = 'providers';
      loadAll().then(() => dialog.showModal()).catch(() => dialog.showModal());
    } else {
      dialog.close();
    }
  });

  async function loadAll() {
    let s;
    try {
      s = await getSettings();
    } catch {
      open = false;
      return;
    }

    try {
      providers = await listProviders();
    } catch {
      providers = [];
    }

    // Ollama quick config — read from the "ollama" provider entry when
    // present, otherwise fall back to the legacy endpoint/model fields.
    const ollamaProv = providers.find(p => p.name === 'ollama');
    ollamaUrl   = ollamaProv?.baseUrl ?? s.endpoint ?? 'http://localhost:11434';
    ollamaModel = s.routing?.interactive?.model ?? s.model ?? 'llama3';

    // Routing
    const r = s.routing ?? {};
    routingInteractiveProvider = r.interactive?.provider ?? '';
    routingInteractiveModel    = r.interactive?.model    ?? '';
    routingBackgroundProvider  = r.background?.provider  ?? '';
    routingBackgroundModel     = r.background?.model     ?? '';
    routingCodingProvider      = r.coding?.provider      ?? '';
    routingCodingModel         = r.coding?.model         ?? '';

    // Channel adapters
    channelAdapters = (s.channelAdapters ?? []).map(a => ({ ...a }));

    // Preferences
    const p = s.preferences ?? {};
    prefAgentName              = p.agentName            ?? 'Pares Agens';
    prefPersonalityNotes       = p.personalityNotes     ?? '';
    prefAutoRecall             = p.autoRecall            ?? true;
    prefCaptureCategories      = p.captureCategories     ?? [];
    prefNotificationsEnabled   = p.notificationsEnabled  ?? true;
    prefAutoStart              = s.autoStart             ?? false;
    prefActivationHotkey       = s.activationHotkey      ?? 'Ctrl+Space';
    prefSystemPrompt           = s.systemPrompt          ?? '';

    // MCP servers
    mcpServers = (s.mcpServers ?? []).map(m => ({ ...m }));
    try {
      mcpTools = await listMcpTools();
    } catch {
      mcpTools = [];
    }

    // License
    try {
      licenseStatus = await getLicenseStatus();
    } catch {
      licenseStatus = { tier: 'free', valid: true };
    }

    // Telemetry
    const t = s.telemetry ?? {};
    telemetryEnabled = t.enabled ?? false;
    telemetryUploadEnabled = t.uploadEnabled ?? false;
    telemetryUploadEndpoint = t.uploadEndpoint ?? '';
    try {
      telemetrySnapshot = await getTelemetrySnapshot();
    } catch {
      telemetrySnapshot = {};
    }
    telemetryUploadError = '';
  }

  // ── Tab keyboard navigation (roving tabindex) ───────────────────────────
  function handleTabKeydown(/** @type {KeyboardEvent} */ e, idx) {
    let next = idx;
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
      next = (idx + 1) % TABS.length;
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
      next = (idx - 1 + TABS.length) % TABS.length;
    } else if (e.key === 'Home') {
      next = 0;
    } else if (e.key === 'End') {
      next = TABS.length - 1;
    } else if (e.key === 'Enter' || e.key === ' ') {
      activeTab = TABS[idx];
      return;
    } else {
      return;
    }
    e.preventDefault();
    activeTab = TABS[next];
    tabButtons[next]?.focus();
  }

  // ── Provider CRUD ────────────────────────────────────────────────────────
  function openAddProvider() {
    editProviderName = null;
    providerFormName = '';
    providerFormUrl  = '';
    providerFormKey  = '';
    providerFormModels = '';
    showProviderForm = true;
  }

  function openEditProvider(/** @type {Provider} */ p) {
    editProviderName   = p.name;
    providerFormName   = p.name;
    providerFormUrl    = p.baseUrl;
    providerFormKey    = '';  // leave blank — backend preserves key when empty
    providerFormModels = (p.models ?? []).join(', ');
    showProviderForm   = true;
  }

  async function saveProvider() {
    const entry = {
      name:    providerFormName.trim(),
      baseUrl: providerFormUrl.trim(),
      apiKey:  providerFormKey.trim() || null,
      models:  providerFormModels.split(',').map(m => m.trim()).filter(Boolean),
    };
    try {
      if (editProviderName === null) {
        await addProvider(entry);
      } else {
        await updateProvider(editProviderName, entry);
      }
      providers = await listProviders();
      showProviderForm = false;
    } catch (err) {
      alert(`Failed to save provider: ${err}`);
    }
  }

  async function deleteProvider(/** @type {string} */ name) {
    if (!confirm(`Remove provider "${name}"?`)) return;
    try {
      await removeProvider(name);
      providers = await listProviders();
    } catch (err) {
      alert(`Failed to remove provider: ${err}`);
    }
  }

  // ── Channel adapter toggle ───────────────────────────────────────────────
  function getAdapter(/** @type {string} */ kind) {
    return channelAdapters.find(a => a.kind === kind);
  }

  function toggleAdapter(/** @type {string} */ kind) {
    const idx = channelAdapters.findIndex(a => a.kind === kind);
    if (idx >= 0) {
      channelAdapters[idx] = { ...channelAdapters[idx], enabled: !channelAdapters[idx].enabled };
    }
  }

  function setAdapterField(/** @type {string} */ kind, /** @type {string} */ field, /** @type {string} */ value) {
    const idx = channelAdapters.findIndex(a => a.kind === kind);
    if (idx >= 0) {
      channelAdapters[idx] = { ...channelAdapters[idx], [field]: value || null };
    }
  }

  // ── Capture category toggle ──────────────────────────────────────────────
  function toggleCategory(/** @type {string} */ cat) {
    if (prefCaptureCategories.includes(cat)) {
      prefCaptureCategories = prefCaptureCategories.filter(c => c !== cat);
    } else {
      prefCaptureCategories = [...prefCaptureCategories, cat];
    }
  }

  // ── Save all ─────────────────────────────────────────────────────────────
  async function saveAll() {
    try {
      // Reload fresh settings to carry over provider list (mutated via separate
      // CRUD commands) and any other fields the UI doesn't manage.
      const fresh = await getSettings();

      // ── Persist Ollama quick-config ────────────────────────────────────
      // Update (or add) the "ollama" provider entry with the URL the user
      // entered, so the model router picks up the change immediately.
      const ollamaBaseUrl   = ollamaUrl.trim()   || 'http://localhost:11434';
      const ollamaModelVal  = ollamaModel.trim()  || 'llama3';
      const existingOllama  = providers.find(p => p.name === 'ollama');
      try {
        if (existingOllama) {
          await updateProvider('ollama', {
              name:    'ollama',
              baseUrl: ollamaBaseUrl,
              apiKey:  null,
              models:  existingOllama.models ?? [ollamaModelVal],
          });
        } else {
          await addProvider({
              name:    'ollama',
              baseUrl: ollamaBaseUrl,
              apiKey:  null,
              models:  [ollamaModelVal],
          });
        }
        // Refresh local provider list after mutation.
        providers = await listProviders();
      } catch (err) {
        console.warn('Failed to update ollama provider entry:', err);
        /* non-fatal — proceed to set_settings */
      }

      // ── Build routing object from UI state ─────────────────────────────
      // Start with the Ollama quick-config as the interactive baseline.
      // If the routing tab has explicit values for both provider and model,
      // those take precedence over the Ollama quick-config.
      const routing = {
        interactive: { provider: 'ollama', model: ollamaModelVal },
      };
      // Routing-tab selections override the Ollama baseline when both fields
      // are filled in (provider name alone is not enough).
      if (routingInteractiveProvider && routingInteractiveModel) {
        routing.interactive = { provider: routingInteractiveProvider, model: routingInteractiveModel };
      }
      if (routingBackgroundProvider && routingBackgroundModel) {
        routing.background = { provider: routingBackgroundProvider, model: routingBackgroundModel };
      }
      if (routingCodingProvider && routingCodingModel) {
        routing.coding = { provider: routingCodingProvider, model: routingCodingModel };
      }

      // Single set_settings call: routing, channel_adapters, preferences, and
      // startup/system-prompt are all written atomically.  Provider CRUD was
      // already applied to the backend state; `fresh` carries those changes.
      // `model` and `endpoint` are also updated for legacy / wizard compat.
      await setSettings({
          ...fresh,
          model:           ollamaModelVal,
          endpoint:        ollamaBaseUrl,
          autoStart:       prefAutoStart,
          activationHotkey: prefActivationHotkey.trim() || 'Ctrl+Space',
          systemPrompt:    prefSystemPrompt,
          routing,
          channelAdapters: channelAdapters,
          mcpServers:      mcpServers,
          preferences: {
            agentName:            prefAgentName,
            personalityNotes:     prefPersonalityNotes,
            autoRecall:           prefAutoRecall,
            captureCategories:    prefCaptureCategories,
            notificationsEnabled: prefNotificationsEnabled,
          },
          telemetry: {
            enabled: telemetryEnabled,
            uploadEnabled: telemetryUploadEnabled,
            uploadEndpoint: telemetryUploadEndpoint.trim() || null,
          },
      });

      open = false;
    } catch (err) {
      alert(`Failed to save settings: ${err}`);
    }
  }

  function handleBackdropClick(e) {
    if (e.target === dialog) open = false;
  }

  // ── MCP server management ───────────────────────────────────────────────
  function openMcpForm(/** @type {McpServer|null} */ server) {
    if (server) {
      editMcpName = server.name;
      mcpFormName = server.name;
      mcpFormCommand = server.command;
      mcpFormArgs = server.args.join(' ');
      mcpFormEnabled = server.enabled;
    } else {
      editMcpName = null;
      mcpFormName = '';
      mcpFormCommand = '';
      mcpFormArgs = '';
      mcpFormEnabled = true;
    }
    showMcpForm = true;
  }

  function saveMcpServer() {
    const server = {
      name: mcpFormName.trim(),
      command: mcpFormCommand.trim(),
      args: mcpFormArgs.trim() ? mcpFormArgs.trim().split(/\s+/) : [],
      enabled: mcpFormEnabled,
    };
    if (!server.name || !server.command) return;

    if (editMcpName) {
      const idx = mcpServers.findIndex(s => s.name === editMcpName);
      if (idx >= 0) mcpServers[idx] = server;
    } else {
      mcpServers.push(server);
    }
    showMcpForm = false;
  }

  function removeMcpServer(/** @type {string} */ name) {
    mcpServers = mcpServers.filter(s => s.name !== name);
  }

  function toggleMcpServer(/** @type {string} */ name) {
    const s = mcpServers.find(s => s.name === name);
    if (s) s.enabled = !s.enabled;
  }

  async function restartMcp() {
    mcpRestarting = true;
    try {
      await restartMcpServers();
      mcpTools = await listMcpTools();
    } catch (err) {
      alert(`MCP restart failed: ${err}`);
    } finally {
      mcpRestarting = false;
    }
  }

  // ── License ──────────────────────────────────────────────────────────────
  async function activateLicense() {
    const key = licenseKey.trim();
    if (!key) {
      licenseError = 'Please enter a license key.';
      return;
    }
    licenseActivating = true;
    licenseError = '';
    try {
      licenseStatus = await activateLicenseApi(key);
      licenseKey = '';
    } catch (err) {
      licenseError = `Activation failed: ${err}`;
    } finally {
      licenseActivating = false;
    }
  }

  function telemetryTodayKey() {
    return new Date().toISOString().slice(0, 10);
  }

  function telemetryModelCallsToday() {
    const byDay = telemetrySnapshot.modelCallsByDay ?? {};
    return byDay[telemetryTodayKey()] ?? 0;
  }

  function telemetryToolEntries() {
    return Object.entries(telemetrySnapshot.toolUsageFrequency ?? {}).sort((a, b) => b[1] - a[1]);
  }

  async function uploadTelemetryNow() {
    telemetryUploading = true;
    telemetryUploadError = '';
    try {
      await uploadTelemetrySnapshotApi();
      telemetrySnapshot = await getTelemetrySnapshot();
    } catch (err) {
      telemetryUploadError = `Upload failed: ${err}`;
    } finally {
      telemetryUploading = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  class="settings-dialog"
  aria-label="Settings"
  onclick={handleBackdropClick}>

  <form method="dialog" onsubmit={(e) => e.preventDefault()}>
    <header class="dialog-header">
      <Text size="lg" weight="bold">Settings</Text>
      <Button variant="ghost" size="sm"
        onclick={() => { open = false; }}>✕</Button>
    </header>

    <!-- Tab bar -->
    <Box role="tablist" aria-label="Settings sections">
      {#each TABS as tab, i}
        <Button
          variant={activeTab === tab ? 'solid' : 'ghost'}
          size="sm"
          onclick={() => { activeTab = tab; }}>
          {tab.charAt(0).toUpperCase() + tab.slice(1)}
        </Button>
      {/each}
    </Box>

    <!-- Providers panel -->
    <Box
      role="tabpanel"
      id="panel-providers"
      aria-labelledby="tab-providers"
      hidden={activeTab !== 'providers'}>

      <!-- Ollama quick configure — visible immediately when Settings opens -->
      <Box border="none">
        <Text weight="bold">Ollama</Text>
        <Input
          label="Endpoint URL"
          bind:value={ollamaUrl}
          placeholder="http://localhost:11434" />
        <Input
          label="Model"
          bind:value={ollamaModel}
          placeholder="llama3" />
        <Text>
          Changes are applied on Save. Run <code>ollama pull {ollamaModel || 'llama3'}</code> to
          ensure the model is available locally.
        </Text>
      </Box>

      <hr class="section-divider" aria-hidden="true" />

      {#if showProviderForm}
        <Box border="none">
          <Text size="lg" weight="bold">{editProviderName === null ? 'Add Provider' : 'Edit Provider'}</Text>
          <Input
            label="Name"
            bind:value={providerFormName}
            placeholder="ollama"
            disabled={editProviderName !== null} />
          <Input
            label="Base URL"
            bind:value={providerFormUrl}
            placeholder="http://localhost:11434" />
          <Input
            label="API Key"
            bind:value={providerFormKey}
            placeholder={editProviderName !== null ? 'unchanged' : 'sk-…'}
            password />
          <Input
            label="Models (comma-separated)"
            bind:value={providerFormModels}
            placeholder="llama3, llama3.1:8b" />
          <Box border="none">
            <Button variant="outline" onclick={() => { showProviderForm = false; }}>Cancel</Button>
            <Button variant="solid" size="sm" onclick={saveProvider}>Save</Button>
          </Box>
        </Box>
      {:else}
        <Box border="none">
          <Button variant="solid" size="sm" onclick={openAddProvider}>+ Add Provider</Button>
        </Box>
        {#if providers.length === 0}
          <Text>No providers configured.</Text>
        {:else}
          <table class="provider-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Base URL</th>
                <th>Key</th>
                <th>Models</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {#each providers as p (p.name)}
                <tr>
                  <td class="provider-name">{p.name}</td>
                  <td class="provider-url">{p.baseUrl}</td>
                  <td class="provider-key">{p.apiKey ? '••••••••' : '—'}</td>
                  <td class="provider-models">{(p.models ?? []).join(', ') || '—'}</td>
                  <td class="provider-actions">
                    <Button variant="ghost" size="sm"
                      onclick={() => openEditProvider(p)}>✎</Button>
                    <Button variant="ghost" size="sm"
                      onclick={() => deleteProvider(p.name)}>✕</Button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      {/if}
    </Box>

    <!-- Routing panel -->
    <Box
      role="tabpanel"
      id="panel-routing"
      aria-labelledby="tab-routing"
      hidden={activeTab !== 'routing'}>

      <Box border="none">
        <Text weight="bold">Route each use-case to a specific provider and model.</Text>

        {#each [
          { label: 'Interactive', providerVal: routingInteractiveProvider, modelVal: routingInteractiveModel,
            setProvider: v => { routingInteractiveProvider = v; }, setModel: v => { routingInteractiveModel = v; } },
          { label: 'Background', providerVal: routingBackgroundProvider, modelVal: routingBackgroundModel,
            setProvider: v => { routingBackgroundProvider = v; }, setModel: v => { routingBackgroundModel = v; } },
          { label: 'Coding', providerVal: routingCodingProvider, modelVal: routingCodingModel,
            setProvider: v => { routingCodingProvider = v; }, setModel: v => { routingCodingModel = v; } },
        ] as row}
          <Box border="none">
            <Text inline>{row.label}</Text>
            <Select
              options={[{value: '', label: '— provider —'}, ...providers.map(p => ({value: p.name, label: p.name}))]}
              value={row.providerVal}
              onchange={(v) => row.setProvider(v)} />
            <Input
              placeholder="model ID"
              value={row.modelVal}
              onchange={(v) => row.setModel(v)} />
          </Box>
        {/each}
      </Box>
    </Box>

    <!-- Channels panel -->
    <Box
      role="tabpanel"
      id="panel-channels"
      aria-labelledby="tab-channels"
      hidden={activeTab !== 'channels'}>

      <Box border="none">
        {#each channelAdapters as adapter (adapter.kind)}
          {@const enabled = adapter.enabled}
          <Box border="none">
            <Box border="none">
              <Text inline>{adapter.kind}</Text>
              <Toggle
                checked={enabled}
                label="Enable {adapter.kind} channel"
                onchange={() => toggleAdapter(adapter.kind)} />
            </Box>

            {#if enabled}
              <Box border="none">
                {#if adapter.kind === 'telegram'}
                  <Input
                    label="Bot Token"
                    password
                    placeholder="123456:ABC-DEF…"
                    value={adapter.botToken ?? ''}
                    onchange={(v) => setAdapterField(adapter.kind, 'botToken', v)} />
                {/if}
                {#if adapter.kind === 'signal'}
                  <Input
                    label="Phone Number"
                    placeholder="+1 555 000 0000"
                    value={adapter.phoneNumber ?? ''}
                    onchange={(v) => setAdapterField(adapter.kind, 'phoneNumber', v)} />
                {/if}
              </Box>
            {/if}
          </Box>
        {/each}
      </Box>
    </Box>

    <!-- Preferences panel -->
    <Box
      role="tabpanel"
      id="panel-preferences"
      aria-labelledby="tab-preferences"
      hidden={activeTab !== 'preferences'}>

      <Box border="none">
        <Text weight="bold">Identity</Text>
        <Input label="Agent Name" bind:value={prefAgentName} placeholder="Pares Agens" />
        <label>
          Personality Notes
          <textarea bind:value={prefPersonalityNotes} rows="3"
            placeholder="Optional notes appended to the system prompt…"></textarea>
        </label>
        <label>
          System Prompt
          <textarea bind:value={prefSystemPrompt} rows="3"></textarea>
        </label>
      </Box>

      <Box border="none">
        <Text weight="bold">Memory</Text>
        <Box border="none">
          <Box border="none">
            <Text inline>Auto-recall</Text>
            <Text inline>Retrieve relevant memories each turn</Text>
          </Box>
          <Toggle label="Enable auto-recall" bind:checked={prefAutoRecall} />
        </Box>
        <Box border="none">
          <Text inline>Capture categories</Text>
          <Box border="none">
            {#each ALL_CAPTURE_CATEGORIES as cat}
              <Toggle
                checked={prefCaptureCategories.includes(cat)}
                label={cat}
                onchange={() => toggleCategory(cat)} />
            {/each}
          </Box>
        </Box>
      </Box>

      <Box border="none">
        <Text weight="bold">Notifications &amp; Startup</Text>
        <Box border="none">
          <Box border="none">
            <Text inline>Desktop notifications</Text>
            <Text inline>Alert when the agent responds</Text>
          </Box>
          <Toggle label="Enable desktop notifications" bind:checked={prefNotificationsEnabled} />
        </Box>
        <Box border="none">
          <Box border="none">
            <Text inline>Launch at login</Text>
            <Text inline>Start minimised to the system tray</Text>
          </Box>
          <Toggle label="Launch at login" bind:checked={prefAutoStart} />
        </Box>
        <Input label="Activation hotkey" bind:value={prefActivationHotkey} placeholder="Ctrl+Space" />
      </Box>
    </Box>

    <!-- MCP panel -->
    <Box
      role="tabpanel"
      id="panel-mcp"
      aria-labelledby="tab-mcp"
      hidden={activeTab !== 'mcp'}>

      <Box border="none">
        <Text size="lg" weight="bold">MCP Servers</Text>
        <Box border="none">
          <Button variant="solid" size="sm" onclick={() => openMcpForm(null)}>
            + Add Server
          </Button>
          <Button variant="outline" size="sm" onclick={restartMcp} disabled={mcpRestarting}>
            {mcpRestarting ? '↻ Restarting…' : '↻ Restart All'}
          </Button>
        </Box>
      </Box>

      {#if showMcpForm}
        <Box border="none">
          <Input label="Name" bind:value={mcpFormName} placeholder="e.g. filesystem" />
          <Input label="Command" bind:value={mcpFormCommand} placeholder="e.g. uvx, npx, node" />
          <Input label="Arguments" bind:value={mcpFormArgs} placeholder="e.g. mcp-server-filesystem /tmp" />
          <Toggle label="Enabled" bind:checked={mcpFormEnabled} />
          <Box border="none">
            <Button variant="solid" size="sm" onclick={saveMcpServer}>
              {editMcpName ? 'Update' : 'Add'}
            </Button>
            <Button variant="outline" size="sm" onclick={() => { showMcpForm = false; }}>
              Cancel
            </Button>
          </Box>
        </Box>
      {/if}

      {#if mcpServers.length === 0}
        <Text>No MCP servers configured. Add one to enable tool use.</Text>
      {:else}
        <Box border="none">
          {#each mcpServers as server}
            <Box border="none">
              <Box border="none">
                <Text inline>{server.name}</Text>
                <code class="mcp-server-cmd">{server.command} {server.args.join(' ')}</code>
              </Box>
              <Box border="none">
                <Button variant="ghost" size="sm" onclick={() => toggleMcpServer(server.name)}>
                  {server.enabled ? '🟢' : '⚪'}
                </Button>
                <Button variant="ghost" size="sm" onclick={() => openMcpForm(server)}>
                  ✏️
                </Button>
                <Button variant="ghost" size="sm" onclick={() => removeMcpServer(server.name)}>
                  🗑
                </Button>
              </Box>
            </Box>
          {/each}
        </Box>
      {/if}

      {#if mcpTools.length > 0}
        <Box border="none">
          <Text weight="bold">Discovered Tools ({mcpTools.length})</Text>
          <Box border="none">
            {#each mcpTools as tool}
              <Box border="none">
                <Text inline>{tool.name}</Text>
                <Text inline>{tool.serverName}</Text>
                {#if tool.description}
                  <Text inline>{tool.description}</Text>
                {/if}
              </Box>
            {/each}
          </Box>
        </Box>
      {/if}
    </Box>

    <!-- Telemetry panel -->
    <Box
      role="tabpanel"
      id="panel-telemetry"
      aria-labelledby="tab-telemetry"
      hidden={activeTab !== 'telemetry'}>

      <Box border="none">
        <Text weight="bold">Privacy-first telemetry</Text>
        <Text>Anonymous metrics only. No conversation content, prompts, tool arguments, or personal identifiers are collected.</Text>

        <Box border="none">
          <Box border="none">
            <Text inline>Enable telemetry (opt-in)</Text>
            <Text inline>Off by default</Text>
          </Box>
          <Toggle label="Enable anonymous telemetry" bind:checked={telemetryEnabled} />
        </Box>

        <Box border="none">
          <Box border="none">
            <Text inline>Enable upload</Text>
            <Text inline>Manual upload of local aggregate metrics</Text>
          </Box>
          <Toggle label="Enable telemetry upload" bind:checked={telemetryUploadEnabled} disabled={!telemetryEnabled} />
        </Box>

        <Input
          label="Upload endpoint"
          bind:value={telemetryUploadEndpoint}
          placeholder="https://example.com/telemetry"
          disabled={!telemetryEnabled || !telemetryUploadEnabled} />
        <Button variant="outline"
          onclick={uploadTelemetryNow}
          disabled={!telemetryEnabled || !telemetryUploadEnabled || !telemetryUploadEndpoint.trim() || telemetryUploading}>
          {telemetryUploading ? 'Uploading…' : 'Upload now'}
        </Button>
        {#if telemetryUploadError}
          <Text>{telemetryUploadError}</Text>
        {/if}
      </Box>

      <Box border="none">
        <Text weight="bold">Local telemetry dashboard</Text>
        <Box border="none">
          <Text inline>Model calls today</Text>
          <strong>{telemetryModelCallsToday()}</strong>
        </Box>
        <Box border="none">
          <Text inline>Avg response latency</Text>
          <strong>{telemetrySnapshot.avgLatencyMs == null ? '—' : `${Math.round(telemetrySnapshot.avgLatencyMs)} ms`}</strong>
        </Box>
        <Box border="none">
          <Text inline>Latency range</Text>
          <strong>{telemetrySnapshot.latencyMinMs == null ? '—' : `${telemetrySnapshot.latencyMinMs}–${telemetrySnapshot.latencyMaxMs} ms`}</strong>
        </Box>
        <Box border="none">
          <Text inline>Samples</Text>
          <strong>{telemetrySnapshot.latencySampleCount ?? 0}</strong>
        </Box>
        <Box border="none">
          <Text inline>Last upload</Text>
          <strong>{telemetrySnapshot.lastUploadAt ? new Date(telemetrySnapshot.lastUploadAt).toLocaleString() : 'Never'}</strong>
        </Box>

        <hr class="section-divider" aria-hidden="true" />
        <Text>Tool usage frequency</Text>
        {#if telemetryToolEntries().length === 0}
          <Text>No tool usage recorded yet.</Text>
        {:else}
          <table class="provider-table">
            <thead>
              <tr>
                <th>Tool</th>
                <th>Calls</th>
              </tr>
            </thead>
            <tbody>
              {#each telemetryToolEntries() as [toolName, count] (toolName)}
                <tr>
                  <td class="provider-name">{toolName}</td>
                  <td>{count}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </Box>
    </Box>

    <!-- License panel -->
    <Box
      role="tabpanel"
      id="panel-license"
      aria-labelledby="tab-license"
      hidden={activeTab !== 'license'}>

      <Box border="none">
        <Text inline>
          {licenseStatus.tier === 'pro' && licenseStatus.valid ? 'Pro' : 'Free'}
        </Text>
        {#if licenseStatus.expires_at}
          <Text inline>Expires: {new Date(licenseStatus.expires_at).toLocaleDateString()}</Text>
        {/if}
      </Box>

      {#if !(licenseStatus.tier === 'pro' && licenseStatus.valid)}
        <Box border="none">
          <Text>Unlock the full power of Pares Agens:</Text>
          <ul class="feature-list">
            <li>✅ Multiple channel adapters</li>
            <li>✅ Multi-provider model routing</li>
            <li>✅ Hyperswarm P2P sync</li>
            <li>✅ MCP tool orchestration</li>
            <li>✅ Praxis audit export</li>
            <li>✅ Procedure editor</li>
          </ul>
        </Box>

        <Box border="none">
          <Input
            label="License Key"
            bind:value={licenseKey}
            placeholder="XXXX-XXXX-XXXX-XXXX"
            onsubmit={activateLicense} />
          {#if licenseError}
            <Text>{licenseError}</Text>
          {/if}
          <Button variant="solid"
            onclick={activateLicense}
            disabled={licenseActivating}>
            {licenseActivating ? 'Activating…' : 'Activate'}
          </Button>
        </Box>
      {:else}
        <Text>Pro features are active. Thank you for your support!</Text>
      {/if}
    </Box>

    <!-- Marketplace panel -->
    <Box
      role="tabpanel"
      id="panel-marketplace"
      aria-labelledby="tab-marketplace"
      hidden={activeTab !== 'marketplace'}>
      <MarketplaceTab />
    </Box>

    <footer class="dialog-footer">
      <Button variant="outline" onclick={() => { open = false; }}>Cancel</Button>
      <Button variant="solid" onclick={saveAll}>Save</Button>
    </footer>
  </form>
</dialog>
