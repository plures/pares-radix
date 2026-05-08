import Procedures from '../Procedures.svelte';

export default {
  id: 'procedures',
  name: 'Procedures',
  iconPath: 'M8 1l-5 8h4l-1 6 5-8H7l1-6z',
  description: 'PluresDB stored procedures',
  version: '1.0.0',
  view: Procedures,
  commands: [
    { id: 'procedures.run', label: 'Run Procedure', action: () => {} },
    { id: 'procedures.create', label: 'Create Procedure', action: () => {} },
  ],
  statusBarItems: [],
};
