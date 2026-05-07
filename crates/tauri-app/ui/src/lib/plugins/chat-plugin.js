import Chat from '../Chat.svelte';

export default {
  id: 'chat',
  name: 'Chat',
  iconPath: 'M2 2h12v8H6l-4 4V2z',
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
