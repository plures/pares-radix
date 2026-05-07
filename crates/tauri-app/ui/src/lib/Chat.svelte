<script>
  import { sendMessage as apiSendMessage, getConversationHistory, listenEvent, isTauri, recordChronos } from './api.js';

  /** @type {{ role: string, content: string }[]} */
  let messages = $state([]);
  let input = $state('');
  let loading = $state(false);
  let messagesEnd;

  $effect(() => {
    if (messagesEnd) messagesEnd.scrollIntoView({ behavior: 'smooth' });
  });

  async function send() {
    const text = input.trim();
    if (!text || loading) return;
    input = '';
    messages = [...messages, { role: 'user', content: text }];
    loading = true;
    recordChronos('ChatSend', 'chat', { length: text.length });
    try {
      const reply = await apiSendMessage(text);
      messages = [...messages, { role: 'assistant', content: reply }];
    } catch (e) {
      messages = [...messages, { role: 'assistant', content: `Error: ${e.message}` }];
    }
    loading = false;
  }

  function handleKeydown(e) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  }
</script>

<div class="chat-plugin">
  <div class="chat-messages">
    {#each messages as msg}
      <div class="chat-msg" class:user={msg.role === 'user'}>
        <p>{msg.content}</p>
      </div>
    {/each}
    {#if loading}
      <div class="chat-msg"><p class="typing">…</p></div>
    {/if}
    <div bind:this={messagesEnd}></div>
  </div>

  <div class="chat-input-row">
    <textarea
      bind:value={input}
      onkeydown={handleKeydown}
      placeholder="Send a message…"
      rows="1"
    ></textarea>
    <button onclick={send} disabled={loading || !input.trim()}>↑</button>
  </div>
</div>

<style>
  .chat-plugin {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .chat-messages {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .chat-msg {
    max-width: 80%;
    padding: 8px 12px;
    border-radius: 8px;
    background: #2a2a3e;
    color: #ccd;
    font-size: 13px;
    line-height: 1.5;
  }
  .chat-msg.user {
    align-self: flex-end;
    background: #364a6b;
  }
  .typing { opacity: 0.5; }
  .chat-input-row {
    display: flex;
    gap: 8px;
    padding: 12px 16px;
    background: #16161e;
  }
  .chat-input-row textarea {
    flex: 1;
    resize: none;
    background: #2a2a3e;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 8px 12px;
    color: #ccd;
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }
  .chat-input-row textarea:focus { border-color: #569cd6; }
  .chat-input-row button {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    border: none;
    background: #569cd6;
    color: #fff;
    font-size: 16px;
    cursor: pointer;
  }
  .chat-input-row button:disabled { opacity: 0.3; cursor: default; }
</style>
