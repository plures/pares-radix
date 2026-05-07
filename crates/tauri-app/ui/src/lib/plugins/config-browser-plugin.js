import ConfigBrowser from '../ConfigBrowser.svelte';

export default {
  id: 'config-browser',
  name: 'Config Browser',
  iconPath: 'M1 1h6v6H1zm8 0h6v6H9zM1 9h6v6H1zm8 0h6v6H9z',
  description: 'Browse PluresDB configuration',
  enabled: true,
  component: ConfigBrowser,
  sidebarComponent: null,
  commands: [],
  settings: {},
};
