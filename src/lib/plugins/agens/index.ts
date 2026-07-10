/**
 * Agens Plugin Registration — plugs the pares-agens agent runtime into pares-radix
 * as an AGENT-type plugin (surface class 'agent', not a panel).
 *
 * This module creates a RadixPlugin that:
 * - Declares itself as an agent surface (manifest type: 'agent')
 * - Contributes its own nav item (💬 Agens → /agent)
 * - Mounts the agent console route (/agent)
 *
 * The agent's cognitive behaviour is declared in the agens praxis module
 * (src/lib/praxis/agens.ts); this binding is the UI/nav surface only. In
 * browser mode the /agent route renders a real "runtime unavailable" empty
 * state (see agentRuntimeAvailable()); in Tauri it wires the real agent-api.
 */

import type { RadixPlugin } from '$lib/types/plugin.js';

export const agensPlugin: RadixPlugin = {
  id: 'agens',
  name: 'Agens',
  version: '0.1.0',
  icon: '💬',
  description: 'Three-agent cognitive loop — the agent console surface',
  type: 'agent',
  dependencies: [],

  // The agent surface is a real SvelteKit route at src/routes/agent/. Nav is
  // registry-derived (navItems below); routes[] is intentionally empty because
  // the console does not dynamically mount agent pages through the plugin
  // route table.
  routes: [],

  navItems: [
    {
      href: '/agent',
      label: 'Agens',
      icon: '💬',
    },
  ],

  settings: [],
  rules: [],
  expectations: [],
  constraints: [],
};

export default agensPlugin;
