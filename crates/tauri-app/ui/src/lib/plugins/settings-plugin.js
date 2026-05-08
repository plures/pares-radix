import Settings from '../Settings.svelte';

export default {
  id: 'settings',
  name: 'Settings',
  iconPath: 'M8 5a3 3 0 100 6 3 3 0 000-6z',
  description: 'Application settings',
  version: '1.0.0',
  view: Settings,
  commands: [
    { id: 'settings.open', label: 'Open Settings', action: () => {} },
  ],
  statusBarItems: [],
};
