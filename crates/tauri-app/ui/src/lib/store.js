// src/lib/store.js
// Application state via @plures/unum reactive bindings
// When PluresDB is connected, this persists across sessions.
// When not connected, falls back to in-memory.

import { writable } from 'svelte/store';

// Application state (will be replaced with Unum when PluresDB backend connects)
export const activeView = writable('chat');
export const sidebarOpen = writable(true);
export const sidebarPanel = writable('memory');
export const commandPaletteOpen = writable(false);
export const panelOpen = writable(false);
export const panelHeight = writable(200);

// Plugin registry
export const plugins = writable([
  { id: 'chat', name: 'Chat', icon: '💬', component: 'Chat', active: true },
  { id: 'procedures', name: 'Procedures', icon: '⚡', component: 'Procedures', active: true },
  { id: 'config-browser', name: 'Config Browser', icon: '🖥️', component: 'ConfigBrowser', active: false },
  { id: 'chronicle', name: 'Timeline', icon: '📜', component: 'Chronicle', active: false },
]);

// Settings
export const settings = writable({
  model: { primary: 'claude-sonnet-4.5', deep: 'claude-opus-4.6', copilot: true },
  theme: 'dark',
  sidebarWidth: 280,
});
