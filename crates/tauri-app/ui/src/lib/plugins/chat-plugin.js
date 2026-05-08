import Chat from '../Chat.svelte';

export default {
  id: 'chat',
  name: 'Chat',
  iconPath: 'M2 2h12v8H6l-4 4V2z',
  description: 'AI assistant chat interface',
  version: '1.0.0',
  view: Chat,
  commands: [
    { id: 'chat.clear', label: 'Clear Chat', action: () => {} },
    { id: 'chat.newSession', label: 'New Chat Session', action: () => {} },
  ],
  statusBarItems: [
    { id: 'chat.model', text: 'claude-sonnet-4.5', position: 'right', priority: 100 },
  ],
};
