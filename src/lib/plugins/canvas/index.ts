/**
 * Canvas Plugin Registration — plugs @plures/canvas-runtime into pares-radix.
 *
 * This module creates a RadixPlugin that:
 * - Registers in the plugin loader
 * - Provides a route (/canvas) with the CanvasRenderer
 * - Adds a nav item (🎨 Canvas)
 * - Exposes MCP tools for AI-driven canvas creation
 * - Adds a dashboard widget showing active canvases
 */

import type { RadixPlugin } from '$lib/types/plugin.js';

export const canvasPlugin: RadixPlugin = {
  id: 'canvas',
  name: 'AI Canvas',
  version: '0.1.0',
  icon: '🎨',
  description: 'Create apps at runtime — AI writes data, apps materialize instantly',
  dependencies: [],

  routes: [
    {
      path: '/',
      title: 'Canvas',
      component: () => import('./CanvasView.svelte'),
    },
  ],

  navItems: [
    {
      href: '/canvas',
      label: 'Canvas',
      icon: '🎨',
    },
  ],

  settings: [
    {
      key: 'canvas.autoSave',
      label: 'Auto-save canvases',
      description: 'Automatically persist canvas changes to PluresDB',
      type: 'toggle',
      default: true,
      group: 'Canvas',
    },
    {
      key: 'canvas.defaultPrefix',
      label: 'PluresDB prefix',
      description: 'Key prefix for canvas data in PluresDB',
      type: 'text',
      default: 'canvas:',
      group: 'Canvas',
    },
  ],

  dashboardWidgets: [
    {
      id: 'canvas.recent',
      title: '🎨 Recent Canvases',
      component: () => import('./CanvasWidgetRecent.svelte'),
      colspan: 1,
      priority: 35,
    },
  ],

  rules: [],
  expectations: [],
  constraints: [],

  async onActivate(ctx) {
    // Register design-dojo components in the canvas runtime registry
    const { registerDesignDojo } = await import('@plures/canvas-runtime');
    await registerDesignDojo();
  },
};

export default canvasPlugin;
