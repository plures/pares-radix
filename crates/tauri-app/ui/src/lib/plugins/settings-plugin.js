import Settings from '../Settings.svelte';

export default {
  id: 'settings',
  name: 'Settings',
  icon: 'settings',
  description: 'Application configuration',
  enabled: true,
  component: Settings,
  sidebarComponent: null,
  commands: [],
  settings: {},
};
