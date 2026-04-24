/**
 * TUI Widget Mapping — design-dojo → svelte-ratatui
 *
 * Maps design-dojo Svelte components to their ratatui widget equivalents.
 * Used by the svelte-ratatui compiler to generate terminal-native rendering
 * and by the TUI CSS theme for browser-based terminal aesthetics.
 *
 * Three rendering modes:
 * 1. GUI (default) — Svelte components render normally in Tauri webview
 * 2. TUI CSS — Same webview but with terminal-aesthetic CSS theme
 * 3. TUI Native — svelte-ratatui compiles components to ratatui widgets
 */


// ─── design-dojo Layout Components ──────────────────────────────────────────

export const layoutMappings: TuiWidgetMapping[] = [
  {
    component: 'Sidebar',
    widget: 'List',
    props: {
      items: 'items',
      currentPath: 'selected_index',
      collapsed: 'minimized',
    },
    tuiLayout: 'left-panel',
  },
  {
    component: 'StatusBar',
    widget: 'Paragraph',
    props: {},
    tuiLayout: 'bottom-bar',
  },
  {
    component: 'StatusBarItem',
    widget: 'Span',
    props: { label: 'text' },
  },
  {
    component: 'TitleBar',
    widget: 'Block',
    props: { title: 'title' },
    tuiLayout: 'top-bar',
  },
  {
    component: 'ActivityBar',
    widget: 'Tabs',
    props: { items: 'titles', activeIndex: 'selected' },
    tuiLayout: 'left-strip',
  },
  {
    component: 'Tabs',
    widget: 'Tabs',
    props: { items: 'titles', activeIndex: 'selected' },
  },
  {
    component: 'EditorTabs',
    widget: 'Tabs',
    props: { tabs: 'titles', activeTab: 'selected' },
  },
  {
    component: 'PluginContentArea',
    widget: 'Block',
    props: {},
    tuiLayout: 'main-content',
  },
];

// ─── design-dojo Primitive Components ───────────────────────────────────────

export const primitiveMappings: TuiWidgetMapping[] = [
  {
    component: 'Button',
    widget: 'Button',
    props: { onclick: 'on_click', disabled: 'disabled', label: 'text' },
  },
  {
    component: 'CommandPalette',
    widget: 'Popup',
    props: { open: 'visible', commands: 'items' },
  },
];

// ─── Design Mode Components ─────────────────────────────────────────────────

export const designModeMappings: TuiWidgetMapping[] = [
  {
    component: 'SchemaExplorer',
    widget: 'Table',
    props: {
      schemas: 'rows',
      selectedSchema: 'selected_row',
    },
  },
  {
    component: 'RuleEditor',
    widget: 'Form',
    props: {
      schema: 'fields',
      draft: 'values',
    },
  },
  {
    component: 'DecisionLedger',
    widget: 'List',
    props: {
      entries: 'items',
    },
  },
];

// ─── All Mappings ───────────────────────────────────────────────────────────

export const allMappings: TuiWidgetMapping[] = [
  ...layoutMappings,
  ...primitiveMappings,
  ...designModeMappings,
];

// ─── TUI Theme (terminal aesthetics) ────────────────────────────────────────

export const tuiTheme = {
  colors: {
    bg: '#0f1117',
    surface: '#1a1d27',
    border: '#2d3140',
    text: '#e2e5eb',
    textMuted: '#8b92a5',
    accent: '#6366f1',
    accentBg: 'rgba(99, 102, 241, 0.15)',
    danger: '#ef4444',
    success: '#22c55e',
    warning: '#f59e0b',
  },
  borders: {
    style: 'rounded' as const,
    chars: { tl: '╭', tr: '╮', bl: '╰', br: '╯', h: '─', v: '│' },
  },
  fonts: {
    mono: "'JetBrains Mono', 'Fira Code', monospace",
  },
};

// ─── Types ──────────────────────────────────────────────────────────────────

export interface TuiWidgetMapping {
  component: string;
  widget: string;
  props: Record<string, string>;
  tuiLayout?: string;
}
