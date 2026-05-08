import Chronicle from '../Chronicle.svelte';

export default {
  id: 'chronicle',
  name: 'Chronicle',
  iconPath: 'M8 1a7 7 0 100 14A7 7 0 008 1zm0 2v5l3 3',
  description: 'Chronos event timeline',
  version: '1.0.0',
  view: Chronicle,
  commands: [
    { id: 'chronicle.refresh', label: 'Refresh Timeline', action: () => {} },
  ],
  statusBarItems: [
    { id: 'chronicle.count', text: '0 events', position: 'right', priority: 50 },
  ],
};
