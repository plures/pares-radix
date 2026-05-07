<script>
  import '@plures/design-dojo/tokens.css';
  import { ChatView, ChatInput, Button, Text, Box } from '@plures/design-dojo';
  import { sendMessage as apiSendMessage, getConversationHistory, listenEvent, readClipboardText, isTauri, recordChronos } from './api.js';

  let { settingsOpen = $bindable(false), proceduresOpen = $bindable(false), agentName = 'Pares Agens' } = $props();

  /**
   * @typedef {import('@plures/design-dojo/dist/app/ChatView.types.js').ChatViewMessage} ChatViewMessage
   */

  /** @type {ChatViewMessage[]} */
  let messages = $state([]);
  let inputValue = $state('');
  let busy = $state(false);
  let connectionState = $state('connected');
  let historyLoaded = $state(false);

  /** Generate unique message ID */
  function msgId() {
    return `msg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  }

  /** Format an ISO timestamp as Date */
  function parseTime(iso) {
    try { return new Date(iso); } catch { return new Date(); }
  }

  // ── Load conversation history from PluresDB on mount ──────────────────
  $effect(() => {
    if (historyLoaded) return;
    getConversationHistory({ channel: 'desktop', limit: 30 })
      .then((history) => {
        if (history && history.length > 0) {
          messages = history.map((m, i) => ({
            id: `hist-${i}`,
            author: m.role === 'user' ? 'You' : m.role === 'system' ? 'System' : agentName,
            content: m.content,
            timestamp: parseTime(m.time),
            type: m.role,
          }));
        }
        historyLoaded = true;
      })
      .catch(() => { historyLoaded = true; });
  });

  // ── Streaming listener ────────────────────────────────────────────────
  $effect(() => {
    const unlistenChunk = listenEvent('model-chunk', ({ request_id, content, done }) => {
      const idx = messages.findLastIndex(m => m.id === request_id);
      if (idx >= 0) {
        if (done) {
          messages[idx] = { ...messages[idx], streaming: false };
        } else {
          messages[idx] = { ...messages[idx], content: messages[idx].content + content };
        }
        messages = [...messages];
      }
    });

    const unlistenResponse = listenEvent('model-response', ({ request_id, content }) => {
      const idx = messages.findLastIndex(m => m.id === request_id);
      if (idx >= 0) {
        messages[idx] = { ...messages[idx], content, streaming: false };
        messages = [...messages];
      } else {
        messages = [...messages, { id: request_id, author: agentName, content, timestamp: new Date(), type: 'agent' }];
      }
      busy = false;
    });

    const unlistenError = listenEvent('model-error', ({ request_id, error }) => {
      const idx = messages.findLastIndex(m => m.id === request_id);
      if (idx >= 0) {
        messages[idx] = { ...messages[idx], content: `⚠️ ${error}`, streaming: false };
        messages = [...messages];
      } else {
        messages = [...messages, { id: msgId(), author: 'System', content: `⚠️ ${error}`, timestamp: new Date(), type: 'system' }];
      }
      busy = false;
    });

    return () => {
      unlistenChunk.then(fn => fn?.());
      unlistenResponse.then(fn => fn?.());
      unlistenError.then(fn => fn?.());
    };
  });

  async function handleSend(content) {
    if (busy || !content?.trim()) return;
    recordChronos('MessageSent', 'chat', { length: content.length });

    const id = msgId();
    busy = true;

    messages = [...messages, { id, author: 'You', content, timestamp: new Date(), type: 'user' }];

    const responseId = `${id}-response`;
    messages = [...messages, { id: responseId, author: agentName, content: '', timestamp: new Date(), type: 'agent', streaming: true }];

    try {
      const response = await apiSendMessage(content, responseId);
      if (response) {
        const idx = messages.findLastIndex(m => m.id === responseId);
        if (idx >= 0 && messages[idx].streaming) {
          messages[idx] = { ...messages[idx], content: response, streaming: false };
          messages = [...messages];
        }
      }
    } catch (err) {
      const idx = messages.findLastIndex(m => m.id === responseId);
      if (idx >= 0) {
        messages[idx] = { ...messages[idx], content: `⚠️ ${err}`, streaming: false };
        messages = [...messages];
      }
    } finally {
      busy = false;
    }
  }

  function clearChat() {
    messages = [];
  }

  $effect(() => {
    const unlisten = listenEvent('show-settings', () => { settingsOpen = true; });
    const unlistenNotificationAction = listenEvent('notification-action', (payload) => {
      const prompt = payload?.prompt;
      if (prompt) { inputValue = prompt; }
    });
    return () => {
      unlisten.then(fn => fn?.());
      unlistenNotificationAction.then(fn => fn?.());
    };
  });
</script>

<Box border="none" class="chat-panel">
  <Box border="none" class="chat-header">
    <Box border="none" class="header-left">
      <Text class="agent-name">{agentName}</Text>
    </Box>
    <Box border="none" class="header-nav">
      <Button variant="ghost" size="sm" onclick={clearChat}>🗑</Button>
      <Button variant="ghost" size="sm" onclick={() => { proceduresOpen = true; }}>⚡</Button>
      <Button variant="ghost" size="sm" onclick={() => { settingsOpen = true; }}>⚙</Button>
    </Box>
  </Box>

  <ChatView
    {messages}
    mode="bubble"
    showTimestamps={true}
  />

  <ChatInput
    bind:value={inputValue}
    placeholder="Type a message… (Enter to send)"
    disabled={busy}
    onsend={handleSend}
  />
</Box>

<style>
  :global(.chat-panel) {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  :global(.chat-header) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.06));
  }

  :global(.header-left) {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  :global(.header-nav) {
    display: flex;
    gap: 4px;
  }

  :global(.agent-name) {
    font-size: 15px;
    font-weight: 600;
  }
</style>


