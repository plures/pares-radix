<script lang="ts">
  import { Box, Text, Heading, Button, TextArea } from '@plures/design-dojo';
  import { onMount } from 'svelte';
  import {
    sendMessage as apiSendMessage,
    getConversationHistory,
    listenEvent,
    type ChatMessage,
  } from '$lib/platform/agent-api.js';

  // eslint-disable-next-line plures/no-raw-stores
  let messages = $state<ChatMessage[]>([]);
  // eslint-disable-next-line plures/no-raw-stores
  let inputValue = $state('');
  // eslint-disable-next-line plures/no-raw-stores
  let isStreaming = $state(false);

  onMount(async () => {
    const history = await getConversationHistory();
    messages = history.map((entry, i) => ({
      id: `history-${i}`,
      role: entry.role as 'user' | 'assistant' | 'system',
      content: entry.content,
      timestamp: entry.timestamp ? new Date(entry.timestamp).getTime() : Date.now(),
      actor: { kind: entry.role === 'user' ? 'human' : 'ai', id: entry.role === 'user' ? 'user:local' : 'ai:agent' },
    }));

    listenEvent('model-error', (payload) => {
      const err = payload as { requestId: string; error: string };
      const idx = messages.findIndex((m) => m.id === err.requestId);
      if (idx !== -1) {
        messages[idx] = { ...messages[idx], content: `Error: ${err.error}`, streaming: false };
        messages = [...messages];
      }
      isStreaming = false;
    });
  });

  async function sendMessage() {
    if (!inputValue.trim() || isStreaming) return;
    const requestId = crypto.randomUUID();
    const userMsg: ChatMessage = {
      id: `user-${requestId}`, role: 'user', content: inputValue.trim(),
      timestamp: Date.now(), actor: { kind: 'human', id: 'user:local' },
    };
    messages = [...messages, userMsg];
    const aiMsg: ChatMessage = {
      id: requestId, role: 'assistant', content: '', timestamp: Date.now(),
      actor: { kind: 'ai', id: 'ai:agent' }, streaming: true,
    };
    messages = [...messages, aiMsg];
    const query = inputValue;
    inputValue = '';
    isStreaming = true;
    try {
      const finalResponse = await apiSendMessage(query, requestId, (chunk) => {
        const idx = messages.findIndex((m) => m.id === requestId);
        if (idx !== -1) {
          messages[idx] = { ...messages[idx], content: messages[idx].content + chunk.text, streaming: !chunk.done };
          messages = [...messages];
        }
      });
      const idx = messages.findIndex((m) => m.id === requestId);
      if (idx !== -1 && !messages[idx].content) {
        messages[idx] = { ...messages[idx], content: finalResponse, streaming: false };
        messages = [...messages];
      } else if (idx !== -1) {
        messages[idx] = { ...messages[idx], streaming: false };
        messages = [...messages];
      }
    } catch (e) {
      const idx = messages.findIndex((m) => m.id === requestId);
      if (idx !== -1) {
        messages[idx] = { ...messages[idx], content: `Error: ${e}`, streaming: false };
        messages = [...messages];
      }
    } finally { isStreaming = false; }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
  }

  function formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
</script>

<svelte:head><title>Chat — Radix</title></svelte:head>

<Box gap="0" class="chat-page">
  <Box as="header" direction="row" justify="space-between" align="center" padding={3}>
    <Heading level={2}>Agent Console</Heading>
    <Button variant="ghost" onclick={() => { messages = []; }}>Clear</Button>
  </Box>

  <Box class="chat-messages" padding={4} gap="12px">
    {#if messages.length === 0}
      <Box align="center" padding={8} gap="12px">
        <Text size="2rem">💬</Text>
        <Text as="p" color="var(--color-text-muted)">Start a conversation with your AI agent.</Text>
      </Box>
    {:else}
      {#each messages as msg (msg.id)}
        <Box class="message {msg.role}" padding={3} gap="4px">
          <Box direction="row" justify="space-between" align="center">
            <Text weight="600" size="0.8rem">{msg.role === 'user' ? '👤 You' : '🤖 Agent'}</Text>
            <Text size="0.75rem" color="var(--color-text-muted)">{formatTime(msg.timestamp)}</Text>
          </Box>
          <Text as="p">{msg.content || '...'}</Text>
          {#if msg.streaming}<Text size="0.75rem" color="var(--color-accent)">●●●</Text>{/if}
        </Box>
      {/each}
    {/if}
  </Box>

  <Box as="footer" direction="row" gap="8px" padding={3} align="end">
    <Box style="flex: 1;">
      <TextArea bind:value={inputValue} placeholder="Type a message..." rows={2} onkeydown={handleKeydown} />
    </Box>
    <Button variant="primary" disabled={!inputValue.trim() || isStreaming} onclick={sendMessage}>Send</Button>
  </Box>
</Box>

<style>
  :global(.chat-page) { height: 100%; display: flex; flex-direction: column; }
  :global(.chat-messages) { flex: 1; overflow-y: auto; }
  :global(.message) { border-radius: 8px; max-width: 80%; }
  :global(.message.user) { background: var(--color-accent, #6366f1); color: #fff; align-self: flex-end; }
  :global(.message.assistant) { background: var(--color-surface-alt, rgba(0,0,0,0.1)); align-self: flex-start; }
</style>
