// src/lib/store.js
// Application state via @plures/unum reactive bindings.
// Backed by PluresDB when connected (Tauri), in-memory adapter otherwise.
// All stores persist across sessions in Tauri mode.

import { persistentStore } from './unum-store.js';

// UI state
export const activeView = persistentStore('radix/ui/activeView', 'chat');
export const sidebarOpen = persistentStore('radix/ui/sidebarOpen', true);
export const sidebarPanel = persistentStore('radix/ui/sidebarPanel', 'memory');
export const commandPaletteOpen = persistentStore('radix/ui/commandPaletteOpen', false);
export const panelOpen = persistentStore('radix/ui/panelOpen', false);
export const panelHeight = persistentStore('radix/ui/panelHeight', 200);

// Plugin registry
export const plugins = persistentStore('radix/plugins', [
  { id: 'chat', name: 'Chat', icon: '💬', component: 'Chat', active: true },
  { id: 'procedures', name: 'Procedures', icon: '⚡', component: 'Procedures', active: true },
  { id: 'config-browser', name: 'Config Browser', icon: '🖥️', component: 'ConfigBrowser', active: false },
  { id: 'chronicle', name: 'Timeline', icon: '📜', component: 'Chronicle', active: false },
]);

// Settings
export const settings = persistentStore('radix/settings', {
  model: { primary: 'claude-sonnet-4.5', deep: 'claude-opus-4.6', copilot: true },
  theme: 'dark',
  sidebarWidth: 280,
});
