<!--
  Chat plugin — Polished chat interface using design-dojo components.
  Wired to the real Rust agent backend via Tauri commands with mock fallback.
-->
<script>
  import { onMount } from 'svelte';
  import { ChatView, ChatInput } from '@plures/design-dojo/app';
  import { sendMessage, getConversationHistory, listenEvent, recordChronos } from './api.js';

  /** @type {import('@plures/design-dojo/app/ChatView.types.js').ChatViewMessage[]} */
  let messages = $state([]);
  let loading = $state(false);
  let msgCounter = $state(0);

  function nextId() {
    msgCounter += 1;
    return `msg-${msgCounter}`;
  }

  onMount(async () => {
    const history = await getConversationHistory();
    messages = history.map((m, i) => ({
      id: `hist-${i}`,
      author: m.role === 'user' ? 'You' : 'Agent',
      content: m.content,
      timestamp: m.time ? new Date(m.time) : new Date(),
      type: m.role === 'user' ? 'user' : 'agent',
    }));

    listenEvent('chat-response', (payload) => {
      const { content } = payload;
      // If we have a streaming placeholder, replace it
      const last = messages[messages.length - 1];
      if (last && last.type === 'agent' && last.streaming) {
        messages[messages.length - 1] = { ...last, content, streaming: false };
        messages = messages;
      } else {
        messages = [...messages, {
          id: nextId(),
          author: 'Agent',
          content,
          timestamp: new Date(),
          type: 'agent',
        }];
      }
      loading = false;
    });
  });

  /**
   * @param {string} value
   */
  async function handleSend(value) {
    if (!value.trim() || loading) return;

    const userMsg = {
      id: nextId(),
      author: 'You',
      content: value,
      timestamp: new Date(),
      type: /** @type {const} */ ('user'),
    };
    messages = [...messages, userMsg];
    loading = true;

    recordChronos('MessageSent', 'chat', { length: value.length });

    // Add streaming placeholder
    const streamId = nextId();
    messages = [...messages, {
      id: streamId,
      author: 'Agent',
      content: '',
      timestamp: new Date(),
      type: /** @type {const} */ ('agent'),
      streaming: true,
    }];

    try {
      const response = await sendMessage(value);
      // If listenEvent already handled it, skip
      const last = messages[messages.length - 1];
      if (last && last.id === streamId && last.streaming) {
        messages[messages.length - 1] = { ...last, content: response, streaming: false };
        messages = messages;
      }
    } catch (e) {
      const last = messages[messages.length - 1];
      if (last && last.id === streamId) {
        messages[messages.length - 1] = { ...last, content: `Error: ${e.message}`, streaming: false, type: 'system' };
        messages = messages;
      }
    }
    loading = false;
  }
</script>

<div class="chat-plugin">
  {#if messages.length === 0 && !loading}
    <div class="chat-empty">
      <span class="chat-empty__text">Start a conversation…</span>
    </div>
  {:else}
    <div class="chat-plugin__messages">
      <ChatView messages={messages} mode="bubble" showTimestamps={false} />
    </div>
  {/if}

  <div class="chat-plugin__input">
    <ChatInput
      placeholder="Send a message…"
      disabled={loading}
      onsend={handleSend}
    />
  </div>
</div>

<style>
  .chat-plugin {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--surface-0, #1e1e2e);
  }

  .chat-plugin__messages {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .chat-plugin__input {
    flex-shrink: 0;
  }

  .chat-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .chat-empty__text {
    color: var(--color-text-muted, #666);
    font-size: var(--text-base, 14px);
    font-style: italic;
  }
</style>
