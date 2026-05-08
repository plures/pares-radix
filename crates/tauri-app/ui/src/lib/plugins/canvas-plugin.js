// Canvas management plugin — provides split/pane commands to the command palette.
import { canvasCommands } from '../store.js';

export default {
  id: 'canvas',
  name: 'Canvas',
  version: '1.0.0',
  iconPath: 'M2 2h5v5H2zM9 2h5v5H9zM2 9h5v5H2zM9 9h5v5H9z',
  // No view — this is a utility plugin (commands only)
  view: null,
  component: null,
  commands: canvasCommands,
};
