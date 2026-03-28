# pares-radix

The Praxis base application — a plugin-driven platform with an inference engine, UX contracts, and LLM integration.

**Radix** (Latin: *root*) is the runtime that turns a bare Svelte+Tauri app into an intelligent, plugin-extensible platform. It provides everything that domain-specific plugins (financial advisor, vault, sprint-log, etc.) need but shouldn't implement themselves.

## What Radix Provides

| Capability | Description |
|---|---|
| **Navigation** | Persistent sidebar, breadcrumbs, mobile responsive, dead-end prevention |
| **Settings** | Unified settings page, plugin settings slots, persistence |
| **Help** | Aggregated help from all active plugins, searchable |
| **Onboarding** | Plugin-aware setup wizard, dependency-ordered steps |
| **Dashboard** | Home page that aggregates plugin widgets |
| **Data** | Import/export orchestration, PluresDB integration |
| **LLM** | Shared provider config, token budgeting, context assembly |
| **Inference** | Praxis rules with confidence scores, decision ledger |
| **UX Contracts** | Journey expectations — prereqs, gates, empty states |
| **Subconscious** | Background preprocessing, context management, rule generation |
| **Plugin Loader** | Discovery, lifecycle, dependency resolution |

## Architecture

```
svelte-tauri-template    →  Generic scaffolding (no opinions)
        ↓
   pares-radix           →  Opinionated runtime (this repo)
        ↓
   pares-modulus          →  Domain plugins (financial, vault, etc.)
```

See [Architecture Doc](docs/architecture.md) for the full design.

## Quick Start

```bash
# Create a new app from template
npx create-plures-app my-app

# Radix is included by default. Add domain plugins:
cd my-app
npx radix plugin add financial-advisor
npx radix plugin add vault
```

## Plugin API

```typescript
import type { RadixPlugin } from '@plures/pares-radix';

export default {
  id: 'my-plugin',
  name: 'My Plugin',
  version: '0.1.0',
  icon: '🔧',

  routes: [
    { path: '/my-plugin', component: () => import('./pages/Home.svelte') },
  ],

  navItems: [
    { href: '/my-plugin', label: 'My Plugin', icon: '🔧' },
  ],

  settings: [
    { key: 'my-plugin.enabled', type: 'toggle', label: 'Enable My Plugin', default: true },
  ],

  expectations: [],
  rules: [],

  async onActivate(ctx) { /* ... */ },
  async onDeactivate() { /* ... */ },
} satisfies RadixPlugin;
```

## Project Status

🚧 **Phase 1 — Foundation** (in progress)

## License

MIT
