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
  /**
   * Optional explicit UI-schema kind. When omitted, the kind is inferred from
   * `category` (see ui-schema.ts `kindForComponent`). Set this only to override
   * the category-based default (e.g. a 'display' component that should behave as
   * a 'container' for layout rules).
   */
  schemaKind?: import('./ui-schema.js').SchemaKind;
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

  // ── New Primitives ────────────────────────────────────────────────────────

  registerComponent('Box', {
    component: dojo.Box as unknown as Component<any>,
    name: 'Box',
    category: 'layout',
    hasChildren: true,
    description: 'Universal layout container. Replaces div, section, article, aside, main, nav, header, footer. Use "as" prop for semantic HTML element.',
    props: [
      { name: 'as', type: 'string', required: false, default: 'div', description: 'HTML element to render' },
      { name: 'padding', type: 'string', required: false, description: 'CSS padding value' },
      { name: 'gap', type: 'string', required: false, description: 'CSS gap value' },
      { name: 'direction', type: "'row' | 'column'", required: false, default: 'column' },
      { name: 'align', type: 'string', required: false, description: 'CSS align-items' },
      { name: 'justify', type: 'string', required: false, description: 'CSS justify-content' },
      { name: 'wrap', type: 'boolean', required: false, default: false },
      { name: 'onclick', type: '() => void', required: false },
    ],
  });

  registerComponent('Text', {
    component: dojo.Text as unknown as Component<any>,
    name: 'Text',
    category: 'display',
    hasChildren: true,
    description: 'Inline or block text. Replaces span, p, em, strong, small, mark. Use "as" prop for element type.',
    props: [
      { name: 'as', type: "'span' | 'p' | 'em' | 'strong' | 'small' | 'mark'", required: false, default: 'span' },
      { name: 'size', type: 'string', required: false, description: 'CSS font-size' },
      { name: 'weight', type: 'string', required: false, description: 'CSS font-weight' },
      { name: 'color', type: 'string', required: false, description: 'CSS color' },
      { name: 'truncate', type: 'boolean', required: false, default: false },
    ],
  });

  registerComponent('Heading', {
    component: dojo.Heading as unknown as Component<any>,
    name: 'Heading',
    category: 'display',
    hasChildren: true,
    description: 'Heading element (h1-h6). Use "level" prop instead of separate h1/h2/h3 elements.',
    props: [
      { name: 'level', type: '1 | 2 | 3 | 4 | 5 | 6', required: false, default: 2, bindable: true },
    ],
  });

  registerComponent('Input', {
    component: dojo.Input as unknown as Component<any>,
    name: 'Input',
    category: 'input',
    hasChildren: false,
    description: 'Text input with label and error display. Replaces input + label elements.',
    props: [
      { name: 'type', type: "'text' | 'number' | 'password' | 'email' | 'url' | 'search'", required: false, default: 'text' },
      { name: 'value', type: 'string', required: false, bindable: true },
      { name: 'placeholder', type: 'string', required: false },
      { name: 'disabled', type: 'boolean', required: false, default: false, bindable: true },
      { name: 'required', type: 'boolean', required: false, default: false },
      { name: 'label', type: 'string', required: false, description: 'Label text above input' },
      { name: 'error', type: 'string', required: false, description: 'Error message below input', bindable: true },
      { name: 'name', type: 'string', required: false },
    ],
  });

  registerComponent('TextArea', {
    component: dojo.TextArea as unknown as Component<any>,
    name: 'TextArea',
    category: 'input',
    hasChildren: false,
    description: 'Multi-line text input with label and error display.',
    props: [
      { name: 'value', type: 'string', required: false, bindable: true },
      { name: 'placeholder', type: 'string', required: false },
      { name: 'rows', type: 'number', required: false, default: 4 },
      { name: 'disabled', type: 'boolean', required: false, default: false, bindable: true },
      { name: 'label', type: 'string', required: false },
      { name: 'error', type: 'string', required: false, bindable: true },
    ],
  });

  registerComponent('Select', {
    component: dojo.Select as unknown as Component<any>,
    name: 'Select',
    category: 'input',
    hasChildren: false,
    description: 'Dropdown select with label. Options provided as array.',
    props: [
      { name: 'value', type: 'string', required: false, bindable: true },
      { name: 'options', type: 'Array<{ value: string, label: string }>', required: true, bindable: true },
      { name: 'placeholder', type: 'string', required: false },
      { name: 'disabled', type: 'boolean', required: false, default: false, bindable: true },
      { name: 'label', type: 'string', required: false },
    ],
  });

  registerComponent('Link', {
    component: dojo.Link as unknown as Component<any>,
    name: 'Link',
    category: 'navigation',
    hasChildren: true,
    description: 'Styled anchor link. Set external=true for new-tab links.',
    props: [
      { name: 'href', type: 'string', required: true, bindable: true },
      { name: 'external', type: 'boolean', required: false, default: false },
    ],
  });

  registerComponent('CodeBlock', {
    component: dojo.CodeBlock as unknown as Component<any>,
    name: 'CodeBlock',
    category: 'display',
    hasChildren: true,
    description: 'Code display block with syntax theme. Replaces pre/code elements.',
    props: [
      { name: 'code', type: 'string', required: false, description: 'Code content (alternative to children)' },
      { name: 'language', type: 'string', required: false, description: 'Language hint for syntax' },
    ],
  });

  registerComponent('List', {
    component: dojo.List as unknown as Component<any>,
    name: 'List',
    category: 'display',
    hasChildren: true,
    description: 'Ordered or unordered list. Wrap ListItem children.',
    props: [
      { name: 'ordered', type: 'boolean', required: false, default: false },
    ],
  });

  registerComponent('ListItem', {
    component: dojo.ListItem as unknown as Component<any>,
    name: 'ListItem',
    category: 'display',
    hasChildren: true,
    description: 'List item. Must be inside a List component.',
    props: [],
  });

  registerComponent('Table', {
    component: dojo.Table as unknown as Component<any>,
    name: 'Table',
    category: 'data',
    hasChildren: true,
    description: 'Styled data table with header row highlighting and hover.',
    props: [],
  });
}

export class ComponentRegistry {
  register = registerComponent;
  resolve = resolveComponent;
  list = listComponents;
  listByCategory = listByCategory;
  catalog = generateCatalog;
}
