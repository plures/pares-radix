/**
 * Component Registry — runtime lookup of components by string name.
 *
 * Pre-registers all design-dojo components. Plugins can register additional
 * components. The CanvasRenderer uses this to resolve component descriptors
 * into actual Svelte components.
 *
 * This is what makes runtime app creation possible: the AI writes
 * { type: "Button", props: { label: "Click me" } } and the registry
 * resolves "Button" to the actual design-dojo Button component.
 */

import type { Component } from 'svelte';

// ── Types ─────────────────────────────────────────────────────────────────────

export interface ComponentMeta {
  /** The Svelte component constructor */
  component: Component<any>;
  /** Human-readable name */
  name: string;
  /** Category for organization in AI/palette UIs */
  category: 'layout' | 'input' | 'display' | 'navigation' | 'feedback' | 'data' | 'custom';
  /** Props schema — what this component accepts */
  props: PropSchema[];
  /** Whether this component can contain children */
  hasChildren: boolean;
  /** Description for AI context */
  description: string;
}

export interface PropSchema {
  /** Prop name */
  name: string;
  /** TypeScript-like type string */
  type: string;
  /** Whether this prop is required */
  required: boolean;
  /** Default value (if any) */
  default?: unknown;
  /** Description for AI context */
  description?: string;
  /** Whether this prop can be bound to a PluresDB key */
  bindable?: boolean;
}

// ── Registry ──────────────────────────────────────────────────────────────────

const registry = new Map<string, ComponentMeta>();

/**
 * Register a component in the runtime registry.
 *
 * @example
 * ```ts
 * import { registerComponent } from '@plures/canvas-runtime/registry';
 * import MyChart from './MyChart.svelte';
 *
 * registerComponent('MyChart', {
 *   component: MyChart,
 *   name: 'My Chart',
 *   category: 'data',
 *   props: [{ name: 'data', type: 'array', required: true, bindable: true }],
 *   hasChildren: false,
 *   description: 'Renders a chart from data array',
 * });
 * ```
 */
export function registerComponent(id: string, meta: ComponentMeta): void {
  registry.set(id, meta);
}

/**
 * Resolve a component by its string ID.
 */
export function resolveComponent(id: string): ComponentMeta | undefined {
  return registry.get(id);
}

/**
 * List all registered components.
 */
export function listComponents(): Array<{ id: string } & ComponentMeta> {
  return [...registry.entries()].map(([id, meta]) => ({ id, ...meta }));
}

/**
 * List components by category.
 */
export function listByCategory(category: ComponentMeta['category']): Array<{ id: string } & ComponentMeta> {
  return listComponents().filter((c) => c.category === category);
}

/**
 * Get the full registry (for inspection/AI context).
 */
export function getRegistry(): ReadonlyMap<string, ComponentMeta> {
  return registry;
}

/**
 * Generate a catalog description suitable for AI context.
 * The AI uses this to know what components are available and how to use them.
 */
export function generateCatalog(): string {
  const lines: string[] = ['# Available Components\n'];
  const categories = new Set(listComponents().map((c) => c.category));

  for (const cat of categories) {
    lines.push(`## ${cat}\n`);
    for (const comp of listByCategory(cat)) {
      lines.push(`### ${comp.id}`);
      lines.push(comp.description);
      lines.push(`Children: ${comp.hasChildren ? 'yes' : 'no'}`);
      if (comp.props.length > 0) {
        lines.push('Props:');
        for (const p of comp.props) {
          const req = p.required ? '(required)' : '(optional)';
          const bind = p.bindable ? ' [bindable]' : '';
          lines.push(`  - ${p.name}: ${p.type} ${req}${bind}${p.description ? ' — ' + p.description : ''}`);
        }
      }
      lines.push('');
    }
  }

  return lines.join('\n');
}

// ── Auto-register design-dojo components ──────────────────────────────────────

/**
 * Register all design-dojo components. Called once at app startup.
 * This makes every design-dojo component available for runtime canvas creation.
 */
export async function registerDesignDojo(): Promise<void> {
  // Import all design-dojo components
  const dojo = await import('@plures/design-dojo');

  // Register each with metadata
  // NOTE: In a real implementation, this metadata would be auto-generated
  // from design-dojo's type definitions at build time. For now, manual.

  registerComponent('Button', {
    component: dojo.Button as unknown as Component<any>,
    name: 'Button',
    category: 'input',
    hasChildren: false,
    description: 'Interactive button with variants (primary, secondary, danger)',
    props: [
      { name: 'label', type: 'string', required: true, bindable: true, description: 'Button text' },
      { name: 'variant', type: "'primary' | 'secondary' | 'danger'", required: false, default: 'primary' },
      { name: 'disabled', type: 'boolean', required: false, default: false, bindable: true },
      { name: 'onclick', type: '() => void', required: false, description: 'Click handler (maps to procedure)' },
    ],
  });

  registerComponent('Dialog', {
    component: dojo.Dialog as unknown as Component<any>,
    name: 'Dialog',
    category: 'feedback',
    hasChildren: false,
    description: 'Modal dialog with confirm/cancel actions',
    props: [
      { name: 'open', type: 'boolean', required: true, bindable: true },
      { name: 'title', type: 'string', required: true, bindable: true },
      { name: 'message', type: 'string', required: true, bindable: true },
      { name: 'confirmLabel', type: 'string', required: false, default: 'OK' },
      { name: 'cancelLabel', type: 'string', required: false, default: 'Cancel' },
      { name: 'onConfirm', type: '() => void', required: true },
      { name: 'onCancel', type: '() => void', required: true },
    ],
  });

  registerComponent('DashboardGrid', {
    component: dojo.DashboardGrid as unknown as Component<any>,
    name: 'Dashboard Grid',
    category: 'layout',
    hasChildren: true,
    description: 'Responsive grid layout for dashboard widgets',
    props: [
      { name: 'widgets', type: 'DashboardWidgetItem[]', required: true, bindable: true },
    ],
  });

  registerComponent('Sidebar', {
    component: dojo.Sidebar as unknown as Component<any>,
    name: 'Sidebar',
    category: 'navigation',
    hasChildren: false,
    description: 'Navigation sidebar with collapsible items',
    props: [
      { name: 'items', type: 'SidebarNavItem[]', required: true, bindable: true },
      { name: 'activeId', type: 'string', required: false, bindable: true },
      { name: 'onSelect', type: '(id: string) => void', required: false },
    ],
  });

  registerComponent('CommandPalette', {
    component: dojo.CommandPalette as unknown as Component<any>,
    name: 'Command Palette',
    category: 'navigation',
    hasChildren: false,
    description: 'Searchable command palette (Ctrl+K style)',
    props: [
      { name: 'open', type: 'boolean', required: true, bindable: true },
      { name: 'commands', type: 'CommandItem[]', required: true, bindable: true },
      { name: 'onSelect', type: '(cmd: CommandItem) => void', required: true },
      { name: 'onClose', type: '() => void', required: true },
    ],
  });

  registerComponent('SettingsPanel', {
    component: dojo.SettingsPanel as unknown as Component<any>,
    name: 'Settings Panel',
    category: 'input',
    hasChildren: false,
    description: 'Auto-generated settings form from schema definitions',
    props: [
      { name: 'groupName', type: 'string', required: true },
      { name: 'settings', type: 'SettingDefinition[]', required: true, bindable: true },
      { name: 'getValue', type: '(key: string) => unknown', required: true },
      { name: 'setValue', type: '(key: string, value: unknown) => void', required: true },
    ],
  });

  registerComponent('StatusBar', {
    component: dojo.StatusBar as unknown as Component<any>,
    name: 'Status Bar',
    category: 'display',
    hasChildren: false,
    description: 'Bottom status bar with left/right item slots',
    props: [
      { name: 'items', type: 'StatusItem[]', required: true, bindable: true },
    ],
  });

  registerComponent('PluginContentArea', {
    component: dojo.PluginContentArea as unknown as Component<any>,
    name: 'Plugin Content Area',
    category: 'layout',
    hasChildren: true,
    description: 'Main content area for plugin views',
    props: [
      { name: 'title', type: 'string', required: false, bindable: true },
    ],
  });
}

export class ComponentRegistry {
  register = registerComponent;
  resolve = resolveComponent;
  list = listComponents;
  listByCategory = listByCategory;
  catalog = generateCatalog;
}
