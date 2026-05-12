# Creating a pares-radix Plugin: The Complete Guide

> How to build, package, register, and distribute a pares-radix plugin. Covers the full lifecycle from empty repo to installed extension.

---

## Architecture Overview

pares-radix plugins follow the **VSCode extension model**: each plugin lives in its own repo, declares its capabilities in a manifest, and gets discovered/installed through a central registry.

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   YOUR PLUGIN REPO (e.g., plures/well-management)          │
│   ├── manifest.toml          ← declares capabilities       │
│   ├── src/                   ← plugin source code           │
│   │   ├── index.ts           ← RadixPlugin default export   │
│   │   ├── pages/             ← Svelte UI components         │
│   │   ├── rules/             ← praxis inference rules       │
│   │   └── stores/            ← domain state                 │
│   └── tests/                 ← plugin tests                 │
│                                                             │
│   pares-modulus (registry)                                  │
│   ├── plugins/well-management/                              │
│   │   ├── manifest.json      ← registry entry (metadata)   │
│   │   └── README.md          ← registry listing docs        │
│   └── registry/index.json   ← auto-generated catalog       │
│                                                             │
│   pares-radix (runtime)                                     │
│   └── radix plugin install well-management                  │
│       → fetches from modulus → loads manifest → activates   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Key principle:** The plugin SOURCE lives in its own repo. The REGISTRY ENTRY (metadata + pointer) lives in pares-modulus. pares-radix discovers and installs from modulus.

This is exactly how VSCode works:
- Your extension code → your GitHub repo
- Marketplace listing → VS Marketplace (analogous to pares-modulus)
- VSCode → downloads and activates (analogous to pares-radix)

---

## Step 1: Create Your Plugin Repo

Your plugin is a standalone repo. It does NOT live inside pares-radix or pares-modulus.

```bash
# Create the repo (or use an existing one like well-management)
gh repo create plures/well-management --public
cd well-management
```

### Required Files

```
well-management/
├── manifest.toml              # Plugin manifest (capabilities, schema, permissions)
├── src/
│   ├── index.ts               # RadixPlugin default export (entry point)
│   ├── pages/                 # Svelte components for UI
│   │   ├── Dashboard.svelte
│   │   └── AssetDetail.svelte
│   ├── rules/                 # Praxis inference rules
│   │   └── water-quality.ts
│   └── stores/                # Domain state (uses PluginContext.data)
│       └── assets.ts
├── tests/                     # Plugin tests
│   └── rules.test.ts
├── package.json               # Dependencies (design-dojo, etc.)
├── README.md
└── LICENSE
```

### manifest.toml — The Plugin Declaration

This is the **single source of truth** for what your plugin does. pares-radix reads this to understand your plugin's capabilities, schema, permissions, and UI.

```toml
[plugin]
name = "well-management"
version = "0.2.0"
description = "Private well system stewardship — testing, treatment, maintenance, compliance"
author = "plures"
license = "MIT"
icon = "💧"
entry = "src/index.ts"
radix = ">=0.2.0"
keywords = ["water", "well", "maintenance", "compliance", "iot"]

# ── Data Schema ────────────────────────────────────────────────

[[schema.entities]]
name = "member"
display_name = "Member"
icon = "👤"

  [[schema.entities.fields]]
  name = "display_name"
  field_type = "String"
  required = true
  description = "Pseudonym or identifier (e.g., 'House 1 Contact')"

  [[schema.entities.fields]]
  name = "role"
  field_type = { Enum = ["viewer", "operator", "treasurer", "admin"] }
  required = true

  [[schema.entities.fields]]
  name = "active"
  field_type = "Boolean"
  required = false

[[schema.entities]]
name = "asset"
display_name = "Asset"
icon = "🔧"

  [[schema.entities.fields]]
  name = "name"
  field_type = "String"
  required = true
  description = "e.g., 'Main Well', 'POE Arsenic System'"

  [[schema.entities.fields]]
  name = "asset_type"
  field_type = { Enum = ["well", "pump", "treatment_system", "storage_tank", "other"] }
  required = true

  [[schema.entities.fields]]
  name = "status"
  field_type = { Enum = ["active", "maintenance", "offline", "decommissioned"] }
  required = true

  [[schema.entities.fields]]
  name = "location_id"
  field_type = "String"
  required = false
  description = "Site identifier (not physical address)"

[[schema.entities]]
name = "test_event"
display_name = "Test Event"
icon = "🧪"

  [[schema.entities.fields]]
  name = "test_type"
  field_type = { Enum = ["bacteria", "chemistry", "flow_rate", "pressure", "visual"] }
  required = true

  [[schema.entities.fields]]
  name = "result_status"
  field_type = { Enum = ["pass", "fail", "pending", "inconclusive"] }
  required = true

  [[schema.entities.fields]]
  name = "tested_date"
  field_type = "Date"
  required = true

  [[schema.entities.fields]]
  name = "notes"
  field_type = "String"
  required = false

[[schema.relationships]]
name = "asset_tests"
from_entity = "test_event"
to_entity = "asset"
cardinality = "many_to_one"

# ── Praxis Logic ────────────────────────────────────────────────

[[logic.rules]]
name = "overdue-test-detection"
description = "Flag assets with no test events in the last 365 days"
condition = "asset.status == 'active' && last_test_event(asset).age_days > 365"
action = "emit warning: '{asset.name} has not been tested in {age} days'"

[[logic.rules]]
name = "bacteria-fail-escalation"
description = "Bacteria test failure triggers immediate retest requirement"
condition = "test_event.test_type == 'bacteria' && test_event.result_status == 'fail'"
action = "create task: 'Retest {asset.name} for bacteria within 48 hours'"

[[logic.constraints]]
name = "no-decommissioned-testing"
description = "Cannot log test events against decommissioned assets"
check = "test_event.asset.status != 'decommissioned'"
error_message = "Cannot test a decommissioned asset. Reactivate it first."

# ── Permissions ────────────────────────────────────────────────

[permissions]
pluresdb_scopes = ["read", "write"]
network = false
tool_access = []

# ── Dependencies ────────────────────────────────────────────────

[dependencies]
plugins = []          # Other pares-radix plugins this depends on
```

### src/index.ts — The Entry Point

This file exports a `RadixPlugin` object, which is the runtime contract between your plugin and pares-radix. Think of it like VSCode's `activate()` function.

```typescript
import type { RadixPlugin, PluginContext } from '@plures/pares-radix';

let ctx: PluginContext;

const wellManagement: RadixPlugin = {
  id: 'well-management',
  name: 'Well Management',
  version: '0.2.0',
  icon: '💧',
  description: 'Private well system stewardship',

  // ── Routes (pages in the app) ──────────────────────────────
  routes: [
    {
      path: '/',
      component: () => import('./pages/Dashboard.svelte'),
      title: 'Well Dashboard',
    },
    {
      path: '/assets',
      component: () => import('./pages/Assets.svelte'),
      title: 'Assets',
    },
    {
      path: '/assets/:id',
      component: () => import('./pages/AssetDetail.svelte'),
      title: 'Asset Detail',
      requires: [{
        type: 'items',
        minCount: 1,
        emptyMessage: 'No assets registered. Add your first well or equipment.',
        fulfillHref: '/well-management/assets',
        fulfillLabel: 'Add Asset',
      }],
    },
    {
      path: '/tests',
      component: () => import('./pages/TestLog.svelte'),
      title: 'Test Log',
    },
    {
      path: '/schedule',
      component: () => import('./pages/Schedule.svelte'),
      title: 'Testing Schedule',
    },
  ],

  // ── Navigation ─────────────────────────────────────────────
  navItems: [
    {
      href: '/well-management',
      label: 'Well Management',
      icon: '💧',
      children: [
        { href: '/well-management/assets', label: 'Assets', icon: '🔧' },
        { href: '/well-management/tests', label: 'Test Log', icon: '🧪' },
        { href: '/well-management/schedule', label: 'Schedule', icon: '📅' },
      ],
    },
  ],

  // ── Settings ───────────────────────────────────────────────
  settings: [
    {
      key: 'well-management.test-reminder-days',
      type: 'number',
      label: 'Test Reminder (days before due)',
      description: 'How many days before a test is due to send a reminder',
      default: 14,
      group: 'Well Management',
    },
    {
      key: 'well-management.bacteria-retest-hours',
      type: 'number',
      label: 'Bacteria Retest Window (hours)',
      description: 'Hours after a bacteria failure before retest is overdue',
      default: 48,
      group: 'Well Management',
    },
  ],

  // ── Dashboard Widgets ──────────────────────────────────────
  dashboardWidgets: [
    {
      id: 'wm-status',
      title: 'Well System Status',
      component: () => import('./widgets/SystemStatus.svelte'),
      colspan: 2,
      priority: 10,
    },
    {
      id: 'wm-upcoming-tests',
      title: 'Upcoming Tests',
      component: () => import('./widgets/UpcomingTests.svelte'),
      colspan: 1,
      priority: 20,
    },
  ],

  // ── Inference Rules ────────────────────────────────────────
  rules: [
    // Import from ./rules/water-quality.ts
  ],

  // ── Lifecycle ──────────────────────────────────────────────
  async onActivate(context: PluginContext) {
    ctx = context;
    // Initialize stores, subscribe to events, etc.
  },

  async onDeactivate() {
    // Cleanup
  },
};

export default wellManagement;
```

---

## Step 2: Develop Locally

During development, you work against a local pares-radix checkout:

```bash
# In your plugin repo
npm install

# Link to local radix for development
npm link @plures/pares-radix

# Or use the CLI to install locally
cd /path/to/pares-radix
cargo run -p pares-agens-cli -- plugin install /path/to/well-management/manifest.toml
```

### Using Platform APIs

Your plugin accesses pares-radix capabilities through `PluginContext`:

```typescript
// Data storage (PluresDB, namespaced to your plugin)
const assets = ctx.data.collection('assets');
await assets.put('well-1', { name: 'Main Well', status: 'active' });
const all = await assets.query({ status: 'active' });

// Settings
const reminderDays = ctx.settings.get<number>('well-management.test-reminder-days');

// Notifications
ctx.notify.warning('Bacteria test due in 3 days for Main Well');

// Navigation
ctx.navigation.goto('/well-management/assets/well-1');

// LLM (token-budgeted, use sparingly)
if (ctx.llm.available()) {
  const summary = await ctx.llm.complete('Summarize water quality trends', { data: testResults });
}
```

### UI Components

All UI must use **design-dojo** components (the pares-radix design system):

```svelte
<!-- pages/Dashboard.svelte -->
<script>
  import { Card, DataTable, Badge, Button } from '@plures/design-dojo';
  import { getContext } from 'svelte';

  const ctx = getContext('plugin');
  let assets = [];

  onMount(async () => {
    assets = await ctx.data.collection('assets').query({});
  });
</script>

<Card title="Well System Overview" icon="💧">
  <DataTable
    columns={[
      { key: 'name', label: 'Asset' },
      { key: 'asset_type', label: 'Type' },
      { key: 'status', label: 'Status', render: (v) => Badge({ text: v, tone: statusTone(v) }) },
    ]}
    rows={assets}
    onRowClick={(row) => ctx.navigation.goto(`/well-management/assets/${row.id}`)}
  />
</Card>
```

---

## Step 3: Register in pares-modulus

Once your plugin is ready, you register it in pares-modulus so others can discover and install it.

### What Goes in Modulus

**Only metadata and a pointer.** Your source code stays in your repo. Modulus holds:

```
pares-modulus/
└── plugins/
    └── well-management/
        ├── manifest.json      ← registry metadata (NOT your full manifest.toml)
        └── README.md          ← listing description
```

### manifest.json (Registry Entry)

This is the **registry listing**, not your full plugin manifest. Think of it like the VS Marketplace listing vs. your actual extension code.

```json
{
  "id": "well-management",
  "name": "Well Management",
  "version": "0.2.0",
  "description": "Private well system stewardship — testing, treatment, maintenance, compliance",
  "author": "plures",
  "license": "MIT",
  "icon": "💧",
  "keywords": ["water", "well", "maintenance", "compliance", "iot"],
  "homepage": "https://github.com/plures/well-management",
  "repository": "https://github.com/plures/well-management",
  "entry": "src/index.ts",
  "radix": ">=0.2.0",
  "dependencies": [],
  "size": {
    "source": "45KB",
    "estimated_bundle": "80KB"
  }
}
```

### Submission Process

```bash
# 1. Fork pares-modulus
gh repo fork plures/pares-modulus --clone
cd pares-modulus

# 2. Create your registry entry
mkdir -p plugins/well-management

# 3. Add manifest.json (registry metadata — see above)
# 4. Add README.md (listing description)

# 5. Validate locally
npx tsx gates/validate-manifest.ts plugins/well-management
npx tsx gates/size-audit.ts plugins/well-management
npx tsx gates/security-scan.ts plugins/well-management

# 6. Submit PR
gh pr create --title "feat: add well-management plugin" --body "..."
```

The PR triggers automated gates:

| Gate | What It Checks |
|---|---|
| **Manifest Validation** | Required fields, id format (kebab-case), version (semver), description ≤200 chars |
| **Size Audit** | Source under 500KB, no binary blobs |
| **Security Scan** | No hardcoded secrets, dependency audit |
| **Radix Compatibility** | Entry file exports valid `RadixPlugin` |
| **Maintainer Review** | Human approval |

On merge, `scripts/build-registry.ts` auto-rebuilds `registry/index.json`.

---

## Step 4: Installation by Users

Once registered, anyone running pares-radix can install your plugin:

```bash
# Browse available plugins
radix plugin browse

# Install
radix plugin install well-management
# → Fetches manifest from modulus registry
# → Clones plugin source from the repo URL in manifest
# → Validates manifest.toml (capability check)
# → Shows permission prompt:
#
#   💧 Well Management wants to:
#   • Read and write to PluresDB (scoped to plugin namespace)
#   • No network access
#   • No tool access
#
#   [Allow] [Deny]
#
# → Registers with PluginRuntime
# → Activates (calls onActivate)

# Update
radix plugin update well-management

# Uninstall
radix plugin uninstall well-management
```

### What Happens at Install Time (Internals)

```
radix plugin install well-management
    │
    ├── 1. Fetch registry/index.json from pares-modulus (GitHub raw or API)
    │      Find entry with id="well-management"
    │      Get repository URL
    │
    ├── 2. Clone/download plugin source from repository
    │      (shallow clone or tarball fetch)
    │
    ├── 3. Parse manifest.toml → PluginManifest (Rust struct)
    │      Validate schema, permissions, dependencies
    │
    ├── 4. Check dependencies (topological sort)
    │      If plugin A depends on B, B must be installed first
    │
    ├── 5. Show capability prompt to user
    │      "This plugin wants: [permissions list]"
    │      User approves or denies
    │
    ├── 6. Register with PluginRuntime
    │      runtime.install(manifest) → stores in HashMap
    │      Generates CRUD tool definitions for entities
    │      Injects schema context into agent system prompt
    │
    ├── 7. Initialize PluresDB namespace
    │      Create plugin-scoped collections for each entity
    │
    └── 8. Call onActivate(ctx)
           Plugin receives PluginContext with all APIs
```

---

## Step 5: The Two Manifest Formats

There are **two** manifest files with different purposes:

| File | Location | Format | Purpose |
|---|---|---|---|
| `manifest.toml` | Your plugin repo | TOML | Full plugin declaration (schema, logic, permissions, UI) |
| `manifest.json` | pares-modulus registry | JSON | Registry listing (metadata + pointer to your repo) |

**manifest.toml** is the rich, complete declaration that pares-radix reads at install time. It includes entity schemas, praxis rules, constraints, permissions, and everything the runtime needs.

**manifest.json** is the lightweight registry entry that modulus uses for discovery. It contains just enough metadata to show in `radix plugin browse` and know where to fetch the source.

When pares-radix installs a plugin, it:
1. Reads `manifest.json` from modulus → gets the repo URL
2. Fetches the plugin source from the repo
3. Reads `manifest.toml` from the plugin repo → gets the full declaration

---

## Step 6: Plugin Security Model (ADR-0011)

All plugins follow the same security rules — no "trusted" first-party exceptions:

1. **Plugins are praxis modules** — all logic is declarative, inspectable, auditable
2. **No direct I/O** — plugins emit events, the platform mediates (no `fetch`, `fs`, `process`)
3. **Capability manifest** — users approve permissions at install time
4. **Restricted runtime** — rule evaluation runs in a frozen context (no `eval`, no `globalThis` mutation)
5. **Namespaced storage** — each plugin gets its own PluresDB namespace, can't read other plugins' data

---

## Common Patterns

### Converting an Existing Repo to a Plugin

If you have an existing repo (like well-management) with docs, runbooks, and templates:

1. **Keep the existing content** — docs, runbooks, templates stay in the repo
2. **Add the plugin layer on top**:
   ```
   well-management/
   ├── docs/                    ← existing docs (unchanged)
   ├── runbooks/                ← existing runbooks (unchanged)
   ├── templates/               ← existing templates (unchanged)
   ├── manifest.toml            ← NEW: plugin declaration
   ├── src/                     ← NEW: plugin source
   │   ├── index.ts
   │   ├── pages/
   │   └── rules/
   └── package.json             ← NEW: dependencies
   ```
3. **Register in modulus** — add the registry entry

The plugin doesn't replace the repo's existing purpose. It extends it with a pares-radix integration.

### Plugin with Backend Crate (Rust)

Some plugins need Rust logic (like Sentinel's constraint engine). The pattern:

```
my-plugin/
├── manifest.toml
├── src/                        ← TypeScript/Svelte (UI + entry point)
├── crates/
│   └── my-plugin-engine/      ← Rust crate (compiled to WASM or Tauri sidecar)
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs
└── package.json
```

The Rust crate compiles to WASM and gets loaded by the plugin at activation. Heavy computation (validation, constraint checking) runs in Rust; UI and integration run in TypeScript.

### Plugin Dependencies

Plugins can depend on other plugins:

```toml
# manifest.toml
[dependencies]
plugins = ["shared-calendar"]   # Must be installed before this plugin
```

pares-radix resolves dependencies via topological sort (Kahn's algorithm, already implemented in `runtime.rs`). Circular dependencies are rejected.

---

## Quick Reference: Plugin Lifecycle

```
┌─────────────┐     ┌──────────────┐     ┌───────────────┐
│   Author     │     │   Register   │     │    Install     │
│              │     │              │     │               │
│ 1. Create    │────▶│ 1. Fork      │────▶│ 1. Browse     │
│    repo      │     │    modulus    │     │    catalog    │
│ 2. Write     │     │ 2. Add       │     │ 2. Install    │
│    manifest  │     │    entry     │     │    command    │
│ 3. Write     │     │ 3. PR +      │     │ 3. Approve    │
│    code      │     │    gates     │     │    perms      │
│ 4. Test      │     │ 4. Merge     │     │ 4. Activate   │
└─────────────┘     └──────────────┘     └───────────────┘
```

---

## Checklist for Plugin Authors

- [ ] Plugin repo exists with `manifest.toml` at root
- [ ] `src/index.ts` exports a valid `RadixPlugin` as default
- [ ] All UI uses design-dojo components
- [ ] Entity schemas defined in `manifest.toml` under `[schema]`
- [ ] Praxis rules/constraints in `manifest.toml` under `[logic]`
- [ ] Permissions declared (minimal — only what's needed)
- [ ] Tests exist and pass
- [ ] `manifest.json` registry entry created in pares-modulus fork
- [ ] All gates pass locally (`validate-manifest`, `size-audit`, `security-scan`)
- [ ] PR submitted to pares-modulus
