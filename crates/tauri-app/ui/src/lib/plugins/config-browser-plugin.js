import ConfigBrowser from '../ConfigBrowser.svelte';

export default {
  id: 'config-browser',
  name: 'Config Browser',
  icon: 'terminal',
  description: 'Browse and validate datacenter configuration',
  enabled: true,
  component: ConfigBrowser,
  sidebarComponent: null,
  commands: [
    { id: 'config.validate', label: 'Validate Configuration', action: 'validate' },
    { id: 'config.import', label: 'Import Config Directory', action: 'import' },
  ],
  settings: {},
};
