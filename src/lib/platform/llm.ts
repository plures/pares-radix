/**
 * LLM Integration Layer — shared provider config and context assembly.
 *
 * Implements the LLMAPI contract defined in plugin.ts.
 * Reads provider configuration from settingsAPI at call time so that
 * changing a setting takes effect immediately without re-creating the API.
 *
 * Supported providers: openai | anthropic | copilot | ollama
 */

import { settingsAPI } from '../stores/settings.js';
import type { LLMAPI } from '../types/plugin.js';

// ─── Constants ────────────────────────────────────────────────────────────────

const DEFAULT_TOKEN_BUDGET = 50_000;
const DEFAULT_OLLAMA_URL = 'http://localhost:11434';

/** Default model per provider, used when radix.llm.model is not set. */
const DEFAULT_MODELS: Record<string, string> = {
  openai: 'gpt-4o-mini',
  anthropic: 'claude-3-haiku-20240307',
  copilot: 'gpt-4o',
  ollama: 'llama3.2',
};

// ─── Session Token Tracking ───────────────────────────────────────────────────

let tokensUsedThisSession = 0;

/** Reset the session token budget counter (call at the start of a new session). */
export function resetTokenBudget(): void {
  tokensUsedThisSession = 0;
}

/** Return the total tokens consumed in the current session. */
export function getTokensUsed(): number {
  return tokensUsedThisSession;
}

// ─── Internal Types ───────────────────────────────────────────────────────────

interface ChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

interface CompletionResult {
  content: string;
  tokensUsed?: number;
}

// ─── Context Assembly ─────────────────────────────────────────────────────────

/**
 * Assemble plugin-contributed context into a structured system message.
 * Each key/value pair is serialised to a labelled line so the LLM has
 * grounded, application-specific context before responding to the prompt.
 */
function assembleSystemMessage(context: Record<string, unknown>): string {
  const entries = Object.entries(context);
  if (entries.length === 0) return '';

  const lines = ['Context provided by the application:'];
  for (const [key, value] of entries) {
    const formatted = typeof value === 'string' ? value : JSON.stringify(value);
    lines.push(`${key}: ${formatted}`);
  }
  return lines.join('\n');
}

// ─── Provider Adapters ────────────────────────────────────────────────────────

async function callOpenAI(
  apiKey: string,
  model: string,
  messages: ChatMessage[],
): Promise<CompletionResult> {
  const res = await fetch('https://api.openai.com/v1/chat/completions', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${apiKey}`,
    },
    body: JSON.stringify({ model, messages }),
  });
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`OpenAI request failed (${res.status}): ${err}`);
  }
  const data = (await res.json()) as {
    choices: { message: { content: string } }[];
    usage?: { total_tokens?: number };
  };
  if (!data.choices?.[0]?.message?.content) {
    throw new Error('OpenAI returned an invalid response structure');
  }
  return {
    content: data.choices[0].message.content,
    tokensUsed: data.usage?.total_tokens,
  };
}

async function callAnthropic(
  apiKey: string,
  model: string,
  messages: ChatMessage[],
): Promise<CompletionResult> {
  const systemParts = messages.filter((m) => m.role === 'system').map((m) => m.content);
  const userMessages = messages
    .filter((m) => m.role !== 'system')
    .map((m) => ({ role: m.role, content: m.content }));

  const res = await fetch('https://api.anthropic.com/v1/messages', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'x-api-key': apiKey,
      'anthropic-version': '2023-06-01',
    },
    body: JSON.stringify({
      model,
      max_tokens: 1024,
      ...(systemParts.length > 0 ? { system: systemParts.join('\n') } : {}),
      messages: userMessages,
    }),
  });
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`Anthropic request failed (${res.status}): ${err}`);
  }
  const data = (await res.json()) as {
    content: { text: string }[];
    usage?: { input_tokens?: number; output_tokens?: number };
  };
  if (!data.content?.[0]?.text) {
    throw new Error('Anthropic returned an invalid response structure');
  }
  const inputTokens = data.usage?.input_tokens ?? 0;
  const outputTokens = data.usage?.output_tokens ?? 0;
  return {
    content: data.content[0].text,
    tokensUsed: inputTokens + outputTokens,
  };
}

async function callGitHubCopilot(
  token: string,
  model: string,
  messages: ChatMessage[],
): Promise<CompletionResult> {
  const res = await fetch('https://api.githubcopilot.com/chat/completions', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
      'editor-version': 'pares-radix/1.0',
      'editor-plugin-version': 'pares-radix/1.0',
    },
    body: JSON.stringify({ model, messages }),
  });
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`GitHub Copilot request failed (${res.status}): ${err}`);
  }
  const data = (await res.json()) as {
    choices: { message: { content: string } }[];
    usage?: { total_tokens?: number };
  };
  if (!data.choices?.[0]?.message?.content) {
    throw new Error('GitHub Copilot returned an invalid response structure');
  }
  return {
    content: data.choices[0].message.content,
    tokensUsed: data.usage?.total_tokens,
  };
}

async function callOllama(
  baseUrl: string,
  model: string,
  messages: ChatMessage[],
): Promise<CompletionResult> {
  const res = await fetch(`${baseUrl}/api/chat`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ model, messages, stream: false }),
  });
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`Ollama request failed (${res.status}): ${err}`);
  }
  const data = (await res.json()) as {
    message: { content: string };
    eval_count?: number;
  };
  return {
    content: data.message.content,
    tokensUsed: data.eval_count,
  };
}

// ─── LLMAPI Factory ───────────────────────────────────────────────────────────

/**
 * Create the platform's LLMAPI implementation.
 *
 * The returned object reads provider configuration from settingsAPI at call
 * time, so updating a setting takes effect on the next `complete()` call
 * without needing to recreate the API instance.
 *
 * Token budget is tracked at the module level and shared across all API
 * instances within a session. Call `resetTokenBudget()` at session start.
 */
export function createLLMAPI(): LLMAPI {
  return {
    available(): boolean {
      const provider = settingsAPI.get<string>('radix.llm.provider');
      return !!provider && provider !== '';
    },

    async complete(prompt: string, context?: Record<string, unknown>): Promise<string> {
      const provider = settingsAPI.get<string>('radix.llm.provider');
      if (!provider || provider === '') {
        throw new Error(
          '[radix] No LLM provider configured. Set a provider in Settings → Platform.',
        );
      }

      const budget =
        settingsAPI.get<number>('radix.llm.tokenBudget') ?? DEFAULT_TOKEN_BUDGET;
      if (budget > 0 && tokensUsedThisSession >= budget) {
        throw new Error(
          `[radix] Session token budget (${budget.toLocaleString()}) exhausted. ` +
            'Start a new session or increase the budget in Settings → Platform.',
        );
      }

      const apiKey = settingsAPI.get<string>('radix.llm.apiKey') ?? '';
      const configuredModel = settingsAPI.get<string>('radix.llm.model');
      const model = configuredModel?.trim() || DEFAULT_MODELS[provider] || '';

      // Context assembly — plugin-contributed context becomes a system message
      const messages: ChatMessage[] = [];
      if (context && Object.keys(context).length > 0) {
        messages.push({ role: 'system', content: assembleSystemMessage(context) });
      }
      messages.push({ role: 'user', content: prompt });

      let result: CompletionResult;
      switch (provider) {
        case 'openai':
          result = await callOpenAI(apiKey, model, messages);
          break;
        case 'anthropic':
          result = await callAnthropic(apiKey, model, messages);
          break;
        case 'copilot':
          result = await callGitHubCopilot(apiKey, model, messages);
          break;
        case 'ollama': {
          const ollamaUrl =
            settingsAPI.get<string>('radix.llm.ollamaUrl')?.replace(/\/$/, '') ??
            DEFAULT_OLLAMA_URL;
          result = await callOllama(ollamaUrl, model, messages);
          break;
        }
        default:
          throw new Error(`[radix] Unknown LLM provider: "${provider}"`);
      }

      if (result.tokensUsed) {
        tokensUsedThisSession += result.tokensUsed;
      }

      return result.content;
    },

    remainingBudget(): number {
      const budget =
        settingsAPI.get<number>('radix.llm.tokenBudget') ?? DEFAULT_TOKEN_BUDGET;
      if (budget === 0) return Number.MAX_SAFE_INTEGER;
      return Math.max(0, budget - tokensUsedThisSession);
    },
  };
}
