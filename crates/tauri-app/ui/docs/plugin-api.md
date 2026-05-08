# Plugin API

Every pares-radix plugin must implement the `RadixPlugin` contract defined in `src/lib/plugins/api.js`.

## Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique plugin identifier |
| `name` | `string` | Human-readable name |
| `iconPath` | `string` | SVG path data (16×16 viewBox) for the activity bar |
| `view` | `SvelteComponent` | Main canvas component |

## Optional Fields

| Field | Type | Description |
|-------|------|-------------|
| `description` | `string` | Short description |
| `version` | `string` | SemVer version |
| `sidebarPanel` | `SvelteComponent` | Sidebar shown when plugin is active |
| `statusBarItems` | `StatusBarContribution[]` | Items contributed to the status bar |
| `commands` | `Command[]` | Commands for the command palette |
| `onActivate` | `(ctx: PluginContext) => void` | Called on activation |
| `onDeactivate` | `() => void` | Called on deactivation |

## StatusBarContribution

```javascript
{ id: 'chat.model', text: 'claude-sonnet-4.5', position: 'right', priority: 100, onclick: () => {} }
```

- `position`: `'left'` (default) or `'right'`
- `priority`: higher = further toward edge

## Command

```javascript
{ id: 'chat.clear', label: 'Clear Chat', keybinding: 'Ctrl+Shift+C', action: () => {} }
```

## PluginContext

Passed to `onActivate()`. Provides platform APIs:

- `notify(message, type)` — Show a toast notification
- `recordEvent(action, data)` — Log a Chronos event
- `getStore(key, defaultValue)` — Access Unum store namespaced to this plugin

## Example Plugin

```javascript
import MyView from '../MyView.svelte';

export default {
  id: 'my-plugin',
  name: 'My Plugin',
  iconPath: 'M2 2h12v12H2z',
  description: 'Does something cool',
  version: '0.1.0',
  view: MyView,
  commands: [
    { id: 'my-plugin.hello', label: 'Say Hello', action: () => console.log('hi') },
  ],
  statusBarItems: [
    { id: 'my-plugin.status', text: 'Ready', position: 'left' },
  ],
  onActivate(ctx) {
    ctx.recordEvent('activated');
    ctx.notify('Plugin loaded!', 'success');
  },
};
```

## Validation

Plugins are validated on registration via `validatePlugin()`. Invalid plugins are rejected with console errors and not loaded.
