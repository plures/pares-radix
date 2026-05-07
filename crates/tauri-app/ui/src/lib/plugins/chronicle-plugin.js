import Chronicle from '../Chronicle.svelte';

export default {
  id: 'chronicle',
  name: 'Timeline',
  icon: 'clock',
  description: 'Chronos event timeline — view all agent decisions and actions',
  enabled: true,
  component: Chronicle,
  sidebarComponent: null,
  commands: [
    { id: 'chronicle.refresh', label: 'Refresh Timeline', action: 'refresh' },
  ],
  settings: {},
};
