// Built-in plugins — auto-registered on startup
import { registerPlugin } from './registry.js';
import chatPlugin from './chat-plugin.js';
import proceduresPlugin from './procedures-plugin.js';
import chroniclePlugin from './chronicle-plugin.js';
import configBrowserPlugin from './config-browser-plugin.js';
import settingsPlugin from './settings-plugin.js';

export function initBuiltinPlugins() {
  registerPlugin(chatPlugin);
  registerPlugin(proceduresPlugin);
  registerPlugin(chroniclePlugin);
  registerPlugin(configBrowserPlugin);
  registerPlugin(settingsPlugin);
}
