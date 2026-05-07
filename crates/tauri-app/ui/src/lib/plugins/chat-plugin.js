import Chat from '../Chat.svelte';

export default {
  id: 'chat',
  name: 'Chat',
  icon: 'chat',
  description: 'AI assistant chat interface',
  enabled: true,
  component: Chat,
  sidebarComponent: null,
  commands: [
    { id: 'chat.clear', label: 'Clear Conversation', action: 'clear' },
    { id: 'chat.model', label: 'Switch Model', action: 'model' },
  ],
  settings: {},
};
