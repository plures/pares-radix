/**
 * Unified API layer — real Tauri invoke in desktop, mock/noop in browser.
 */

const isTauri = typeof window !== 'undefined' && !!window.__TAURI__;
const tauriCore = isTauri ? window.__TAURI__.core : null;
const tauriEvent = isTauri ? window.__TAURI__.event : null;

/** @param {string} cmd @param {Record<string, unknown>} [args] */
async function invoke(cmd, args) {
  if (!tauriCore) {
    console.debug(`[api] invoke('${cmd}') — no Tauri runtime, returning mock`);
    return null;
  }
  return tauriCore.invoke(cmd, args);
}

// ── Window controls ────────────────────────────────────────────────────────

export async function minimizeWindow() {
  if (!isTauri) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  getCurrentWindow().minimize();
}

export async function maximizeWindow() {
  if (!isTauri) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  getCurrentWindow().toggleMaximize();
}

export async function closeWindow() {
  if (!isTauri) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  getCurrentWindow().close();
}

// ── Event listening ────────────────────────────────────────────────────────

/**
 * @param {string} event
 * @param {(event: { payload: any }) => void} handler
 * @returns {Promise<() => void>} unlisten function
 */
export async function listen(event, handler) {
  if (!tauriEvent) return () => {};
  return tauriEvent.listen(event, handler);
}

// ── Notifications ──────────────────────────────────────────────────────────

/** @param {{ notificationId: string, action: string }} params */
export async function handleNotificationAction(params) {
  return invoke('handle_notification_action', params);
}

// ── Settings ───────────────────────────────────────────────────────────────

export async function getSettings() {
  const result = await invoke('get_settings');
  return result ?? {
    routing: {},
    channelAdapters: [],
    preferences: {},
    mcpServers: [],
    telemetry: {},
  };
}

/** @param {Record<string, unknown>} settings */
export async function setSettings(settings) {
  return invoke('set_settings', { settings });
}

// ── Providers ──────────────────────────────────────────────────────────────

export async function listProviders() {
  return (await invoke('list_providers')) ?? [];
}

export async function addProvider(provider) {
  return invoke('add_provider', { provider });
}

export async function updateProvider(name, provider) {
  return invoke('update_provider', { name, provider });
}

export async function removeProvider(name) {
  return invoke('remove_provider', { name });
}

// ── MCP ────────────────────────────────────────────────────────────────────

export async function listMcpTools() {
  return (await invoke('list_mcp_tools')) ?? [];
}

export async function restartMcpServers() {
  return invoke('restart_mcp_servers');
}

// ── Procedures ─────────────────────────────────────────────────────────────

export async function listProcedures() {
  return (await invoke('list_procedures')) ?? [];
}

export async function getProcedure(name) {
  return invoke('get_procedure', { name });
}

export async function getProcedureLog(name, limit = 50) {
  return (await invoke('get_procedure_log', { name, limit })) ?? [];
}

export async function toggleProcedure(name, enabled) {
  return invoke('toggle_procedure', { name, enabled });
}

export async function saveProcedure(record) {
  return invoke('save_procedure', { record });
}

export async function createFromTemplate(template) {
  return invoke('create_from_template', { template });
}

// ── Wizard ─────────────────────────────────────────────────────────────────

export async function completeWizard(settings, swarm) {
  return invoke('complete_wizard', { settings, swarm });
}

export async function detectDockerRunner() {
  return (await invoke('detect_docker_runner')) ?? false;
}

export async function validateApiKey(provider, apiKey) {
  return (await invoke('validate_api_key', { provider, apiKey })) ?? false;
}

export async function generateSwarmInvite() {
  return (await invoke('generate_swarm_invite')) ?? { topic: '', sharedKey: '' };
}

export async function verifySwarmJoin(topic, sharedKey) {
  return invoke('verify_swarm_join', { topic, sharedKey });
}

// ── License ────────────────────────────────────────────────────────────────

export async function getLicenseStatus() {
  return (await invoke('get_license_status')) ?? { tier: 'free', valid: true };
}

export async function activateLicense(key) {
  return invoke('activate_license', { key });
}

// ── Telemetry ──────────────────────────────────────────────────────────────

export async function getTelemetrySnapshot() {
  return (await invoke('get_telemetry_snapshot')) ?? {};
}

export async function uploadTelemetrySnapshot() {
  return invoke('upload_telemetry_snapshot');
}

// ── Marketplace ────────────────────────────────────────────────────────────

export async function marketplaceSearch(query) {
  return (await invoke('marketplace_search', { query })) ?? [];
}

export async function marketplaceListInstalled() {
  return (await invoke('marketplace_list_installed')) ?? [];
}

export async function marketplaceCheckUpdates() {
  return (await invoke('marketplace_check_updates')) ?? [];
}

export async function marketplaceInstall(id) {
  return invoke('marketplace_install', { id });
}

export async function marketplaceRemove(id) {
  return invoke('marketplace_remove', { id });
}

export async function marketplaceUpdateAll() {
  return invoke('marketplace_update_all');
}

// ── Config Browser ─────────────────────────────────────────────────────────

export async function getConfigTree() {
  return (await invoke('config_tree', {})) ?? {};
}
