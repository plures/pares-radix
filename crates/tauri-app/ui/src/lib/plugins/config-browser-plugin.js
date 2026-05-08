import ConfigBrowser from '../ConfigBrowser.svelte';

export default {
  id: 'config-browser',
  name: 'Config Browser',
  iconPath: 'M1 1h6v6H1zm8 0h6v6H9zM1 9h6v6H1zm8 0h6v6H9z',
  description: 'Browse PluresDB configuration',
  version: '1.0.0',
  view: ConfigBrowser,
  commands: [
    { id: 'config.refresh', label: 'Refresh Config', action: () => {} },
  ],
  statusBarItems: [],
};
