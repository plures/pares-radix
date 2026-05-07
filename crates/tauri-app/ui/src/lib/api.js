// src/lib/api.js — Unified Tauri API abstraction layer.
// In Tauri: calls real backend commands.
// In browser: returns mock data for development.

const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;
export const isTauri = !!invoke;

// ── Chat ────────────────────────────────────────────────────────────────────

/**
 * Send a chat message to the agent and get a response.
 * @param {string} content - The message text
 * @param {string} requestId - Unique ID to correlate streaming chunks
 * @returns {Promise<string>}
 */
export async function sendMessage(content, requestId) {
  if (isTauri) {
    return await invoke('send_message', { content, requestId });
  }
  await new Promise(r => setTimeout(r, 1000));
  return `[Mock response to: "${content}"]`;
}

/**
 * Get conversation history.
 * @param {{ channel?: string, limit?: number }} opts
 * @returns {Promise<Array<{role: string, content: string, time: string}>>}
 */
export async function getConversationHistory({ channel = 'desktop', limit = 30 } = {}) {
  if (isTauri) {
    return await invoke('get_conversation_history', { channel, limit }) || [];
  }
  return [];
}

/**
 * Listen for Tauri events (streaming chunks, responses, errors).
 * Returns an unlisten function. No-op in browser mode.
 * @param {string} event
 * @param {(payload: any) => void} handler
 * @returns {Promise<() => void>}
 */
export async function listenEvent(event, handler) {
  if (isTauri && listen) {
    return await listen(event, (e) => handler(e.payload));
  }
  return () => {};
}

// ── Terminal ────────────────────────────────────────────────────────────────

/**
 * Run a shell command.
 * @param {string} command
 * @returns {Promise<string>}
 */
export async function runCommand(command) {
  if (isTauri) {
    return await invoke('run_shell_command', { command });
  }
  return `[Mock] Would execute: ${command}`;
}

// ── Chronicle / Chronos ─────────────────────────────────────────────────────

/**
 * Get recent Chronos entries.
 * @param {number} limit
 * @returns {Promise<Array>}
 */
export async function getChronosEntries(limit = 50) {
  if (isTauri) {
    return await invoke('chronos_recent', { limit }) || [];
  }
  return [
    { id: '1', timestamp: new Date().toISOString(), action: 'MessageReceived', key: 'tui:user', data: { content: 'hello' } },
    { id: '2', timestamp: new Date().toISOString(), action: 'ModelCalled', key: 'copilot:claude-sonnet-4.5', data: { latency_ms: 2100 } },
    { id: '3', timestamp: new Date().toISOString(), action: 'ResponseGenerated', key: 'tui:agent', data: { length: 42 } },
  ];
}

// ── Memory ──────────────────────────────────────────────────────────────────

/**
 * Get recent memories for the sidebar.
 * @returns {Promise<Array<{id: string, content: string, category: string, created_at: string}>>}
 */
export async function getMemories() {
  if (isTauri) {
    return await invoke('get_memories');
  }
  return [{ id: '1', content: 'Mock memory result', category: 'note', created_at: new Date().toISOString() }];
}

// ── Praxis Guidance ─────────────────────────────────────────────────────────

/**
 * Get Praxis guidance entries for a category.
 * @param {string} category - facts|rules|constraints|decisions|risks|guidance
 * @returns {Promise<Array>}
 */
export async function getPraxisGuidance(category) {
  if (isTauri) {
    return await invoke('get_praxis_guidance', { category });
  }
  return [];
}

/**
 * Get recent analysis events.
 * @param {number} limit
 * @returns {Promise<Array>}
 */
export async function getAnalysisEvents(limit = 10) {
  if (isTauri) {
    return await invoke('get_analysis_events', { limit });
  }
  return [];
}

/**
 * Trigger Praxis analysis manually.
 * @returns {Promise<number>} Number of memories analyzed
 */
export async function triggerPraxisAnalysis() {
  if (isTauri) {
    return await invoke('trigger_praxis_analysis');
  }
  return 0;
}

/**
 * Get source spans for traceability.
 * @param {string[]} spanIds
 * @returns {Promise<Array>}
 */
export async function getSourceSpans(spanIds) {
  if (isTauri) {
    return await invoke('get_source_spans', { spanIds });
  }
  return [];
}

// ── Settings ────────────────────────────────────────────────────────────────

/**
 * Get current application settings.
 * @returns {Promise<object>}
 */
export async function getSettings() {
  if (isTauri) {
    return await invoke('get_settings');
  }
  return { model: 'claude-sonnet-4.5', system_prompt: '', auto_start: false };
}

/**
 * Save application settings.
 * @param {object} settings
 * @returns {Promise<void>}
 */
export async function setSettings(settings) {
  if (isTauri) {
    return await invoke('set_settings', { settings });
  }
}

// ── Clipboard (Tauri plugin) ────────────────────────────────────────────────

/**
 * Read clipboard text (re-export for convenience).
 * Falls back to empty string in browser mode.
 * @returns {Promise<string>}
 */
export async function readClipboardText() {
  if (isTauri) {
    const { readText } = await import('@tauri-apps/plugin-clipboard-manager');
    const value = await readText();
    return (value ?? '').replace(/\r\n/g, '\n');
  }
  return '';
}
