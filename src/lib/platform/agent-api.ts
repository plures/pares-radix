/**
 * Agent API — bridges the Svelte frontend to the pares-agens Rust backend.
 *
 * All agent operations go through this module. It wraps Tauri IPC commands
 * with browser-safe fallbacks for development outside Tauri.
 *
 * Architecture:
 * - In Tauri: Svelte → invoke() → Rust agent runtime → model API → response
 * - In browser: no agent runtime — calls return a typed capability-unavailable
 *   signal so callers render a real "desktop only" empty-state (no fake chat).
 *
 * Every call flows through PluresDB for persistence and Chronos for logging.
 */

import { browser } from '$app/environment';

// ── Types ─────────────────────────────────────────────────────────────────────

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
  actor: { kind: string; id: string };
  streaming?: boolean;
}

export interface ToolCall {
  name: string;
  arguments: Record<string, unknown>;
}

export interface ModelChunkEvent {
  requestId: string;
  text: string;
  done: boolean;
}

export interface ConversationEntry {
  role: string;
  content: string;
  timestamp?: string;
}

// ── Tauri Bridge ──────────────────────────────────────────────────────────────

let invoke: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null = null;
let listen: ((event: string, handler: (e: { payload: unknown }) => void) => Promise<() => void>) | null = null;

async function ensureTauri(): Promise<boolean> {
  if (!browser) return false;
  // Honest IPC-host detection: the @tauri-apps/api/core module RESOLVES in a plain
  // browser (it's an installed dep), so a successful import must NOT be treated as
  // "runtime available". Only a real Tauri window injects __TAURI_INTERNALS__. Without
  // this guard the browser falsely reports the agent runtime present and mounts a
  // hollow, non-functional chat bridge (C-NOSTUB-001 violation).
  if (!('__TAURI_INTERNALS__' in window)) return false;
  if (invoke) return true;

  try {
    const core = await import('@tauri-apps/api/core');
    const event = await import('@tauri-apps/api/event');
    invoke = core.invoke;
    listen = event.listen;
    return true;
  } catch {
    return false;
  }
}

// ── Capability Detection ────────────────────────────────────────────────────────

/**
 * Whether the real agent runtime (Tauri IPC bridge) is available in this
 * render context. Wraps ensureTauri(). In browser mode this is false and the
 * agent surface must render a real "desktop only" empty-state rather than fake
 * a conversation.
 */
export async function agentRuntimeAvailable(): Promise<boolean> {
  return ensureTauri();
}

/**
 * Typed signal returned by sendMessage when no agent runtime is available.
 * Callers must handle this explicitly; it is NOT a fabricated response.
 */
export interface AgentRuntimeUnavailable {
  unavailable: true;
  capability: 'agent-runtime';
}

/** Type guard for the capability-unavailable signal. */
export function isAgentRuntimeUnavailable(
  v: unknown,
): v is AgentRuntimeUnavailable {
  return (
    typeof v === 'object' &&
    v !== null &&
    (v as AgentRuntimeUnavailable).unavailable === true &&
    (v as AgentRuntimeUnavailable).capability === 'agent-runtime'
  );
}

// ── Chat API ──────────────────────────────────────────────────────────────────

/**
 * Send a message to the agent and get a response.
 *
 * In Tauri mode: calls send_message command, which goes through the full
 * agent runtime (cerebellum → model → tool calls → response). Returns the
 * final response text. For streaming, use onChunk callback.
 *
 * In browser mode: no agent runtime exists — returns a typed
 * AgentRuntimeUnavailable signal instead of fabricating a reply.
 */
export async function sendMessage(
  content: string,
  requestId: string,
  onChunk?: (chunk: ModelChunkEvent) => void,
): Promise<string | AgentRuntimeUnavailable> {
  const hasTauri = await ensureTauri();

  if (hasTauri && invoke && listen) {
    // Set up streaming listener before sending
    let unlisten: (() => void) | undefined;

    if (onChunk) {
      unlisten = await listen('model-chunk', (e) => {
        const payload = e.payload as ModelChunkEvent;
        if (payload.requestId === requestId) {
          onChunk(payload);
        }
      }) as unknown as () => void;
    }

    try {
      const result = await invoke('send_message', { content, requestId });
      return result as string;
    } finally {
      unlisten?.();
    }
  }

  // No agent runtime (browser mode) — real capability-unavailable signal.
  return { unavailable: true, capability: 'agent-runtime' };
}

/**
 * Get conversation history.
 */
export async function getConversationHistory(
  channel: string = 'default',
  limit: number = 50,
): Promise<ConversationEntry[]> {
  const hasTauri = await ensureTauri();

  if (hasTauri && invoke) {
    try {
      return (await invoke('get_conversation_history', { channel, limit })) as ConversationEntry[];
    } catch {
      return [];
    }
  }

  return [];
}

// ── Memory API ────────────────────────────────────────────────────────────────

/**
 * Get recent memories from PluresLM.
 */
export async function getMemories(): Promise<unknown[]> {
  const hasTauri = await ensureTauri();

  if (hasTauri && invoke) {
    try {
      return (await invoke('get_memories')) as unknown[];
    } catch {
      return [];
    }
  }

  return [];
}

// ── MCP Tools API ─────────────────────────────────────────────────────────────

export interface DiscoveredTool {
  serverName: string;
  name: string;
  description: string | null;
  inputSchema: unknown;
}

/**
 * List all available MCP tools.
 */
export async function listMcpTools(): Promise<DiscoveredTool[]> {
  const hasTauri = await ensureTauri();

  if (hasTauri && invoke) {
    try {
      return (await invoke('list_mcp_tools')) as DiscoveredTool[];
    } catch {
      return [];
    }
  }

  return [];
}

/**
 * Call an MCP tool.
 */
export async function callMcpTool(
  serverName: string,
  toolName: string,
  args: Record<string, unknown>,
): Promise<{ content: string; isError: boolean }> {
  const hasTauri = await ensureTauri();

  if (hasTauri && invoke) {
    try {
      const result = await invoke('call_mcp_tool', {
        serverName,
        toolName,
        arguments: JSON.stringify(args),
      });
      return result as { content: string; isError: boolean };
    } catch (e) {
      return { content: String(e), isError: true };
    }
  }

  return { content: 'MCP not available in browser mode', isError: true };
}

// ── Settings API ──────────────────────────────────────────────────────────────

/**
 * Get current agent settings.
 */
export async function getSettings(): Promise<unknown> {
  const hasTauri = await ensureTauri();

  if (hasTauri && invoke) {
    try {
      return await invoke('get_settings');
    } catch {
      return {};
    }
  }

  return {};
}

/**
 * Update agent settings.
 */
export async function setSettings(settings: unknown): Promise<void> {
  const hasTauri = await ensureTauri();

  if (hasTauri && invoke) {
    await invoke('set_settings', { settings });
  }
}

// ── Praxis API ────────────────────────────────────────────────────────────────

/**
 * Get praxis guidance for a topic.
 */
export async function getPraxisGuidance(topic: string): Promise<unknown[]> {
  const hasTauri = await ensureTauri();

  if (hasTauri && invoke) {
    try {
      return (await invoke('get_praxis_guidance', { topic })) as unknown[];
    } catch {
      return [];
    }
  }

  return [];
}

/**
 * Trigger a full praxis analysis.
 */
export async function triggerPraxisAnalysis(): Promise<number> {
  const hasTauri = await ensureTauri();

  if (hasTauri && invoke) {
    try {
      return (await invoke('trigger_praxis_analysis')) as number;
    } catch {
      return 0;
    }
  }

  return 0;
}

// ── Telemetry API ─────────────────────────────────────────────────────────────

/**
 * Get a telemetry snapshot (Chronos).
 */
export async function getTelemetrySnapshot(): Promise<unknown> {
  const hasTauri = await ensureTauri();

  if (hasTauri && invoke) {
    try {
      return await invoke('get_telemetry_snapshot');
    } catch {
      return null;
    }
  }

  return null;
}

// ── Event Listening ───────────────────────────────────────────────────────────

/**
 * Listen for Tauri events from the backend.
 * Returns an unsubscribe function.
 */
export async function listenEvent(
  event: string,
  handler: (payload: unknown) => void,
): Promise<() => void> {
  const hasTauri = await ensureTauri();

  if (hasTauri && listen) {
    const unlisten = await listen(event, (e) => handler(e.payload));
    return unlisten as unknown as () => void;
  }

  return () => {};
}
