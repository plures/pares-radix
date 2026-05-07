import Settings from '../Settings.svelte';

export default {
  id: 'settings',
  name: 'Settings',
  iconPath: 'M8 5a3 3 0 100 6 3 3 0 000-6z',
  description: 'Application settings',
  enabled: true,
  component: Settings,
  sidebarComponent: null,
  commands: [],
  settings: {},
};
