import type { SvelteComponent, Snippet } from 'svelte';
import type { SchemaDelta } from './schema-delta.js';
export type { SchemaDelta } from './schema-delta.js';

export interface DashboardWidgetItem {
  id: string;
  title: string;
  component: () => Promise<{ default: typeof SvelteComponent }>;
  colspan?: number;
  priority?: number;
}

export interface DashboardGridProps {
  widgets: DashboardWidgetItem[];
}

export interface WizardStep {
  title: string;
  description: string;
  icon: string;
  href: string;
  actionLabel: string;
  isComplete: () => boolean | Promise<boolean>;
  after?: string[];
}

export interface FirstRunWizardProps {
  steps: WizardStep[];
  isComplete: (stepTitle: string) => boolean;
  markComplete: (stepTitle: string) => void;
}

export type SettingInputType = 'toggle' | 'select' | 'text' | 'number' | 'password' | 'color';
export interface SettingDefinition { key: string; type: SettingInputType; label: string; description?: string; default: unknown; options?: { value: string; label: string }[]; group?: string; }
export interface SettingsPanelProps { groupName: string; settings: SettingDefinition[]; getValue: (key: string) => unknown; setValue: (key: string, value: unknown) => void; }
export interface ButtonProps { variant?: 'primary' | 'secondary' | 'danger' | 'ghost'; disabled?: boolean; onclick?: (e: MouseEvent) => void; class?: string; }
export interface SidebarNavItem { href: string; label: string; icon?: string; badge?: number; }
export interface SidebarProps { items: SidebarNavItem[]; currentPath: string; collapsed?: boolean; onToggle?: () => void; }
export interface CommandItem { id: string; label: string; icon?: string; action: () => void; }
export interface CommandPaletteProps { open?: boolean; commands?: CommandItem[]; onClose?: () => void; }
export interface StatusItem { label: string; value: string; }
export interface StatusBarProps { items?: StatusItem[]; }
export interface PluginContentAreaProps { theme?: string; onThemeToggle?: () => void; onSidebarToggle?: () => void; onCommandPaletteOpen?: () => void; statusItems?: StatusItem[]; children: Snippet; }

export interface DialogProps {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export interface InputProps {
  type?: string;
  value?: string | number;
  placeholder?: string;
  disabled?: boolean;
  required?: boolean;
  checked?: boolean;
  name?: string;
  label?: string;
  error?: string;
  class?: string;
  oninput?: (e: Event) => void;
  onchange?: (e: Event) => void;
  min?: number | string;
  max?: number | string;
  step?: number | string;
  accept?: string;
  "aria-label"?: string;
  onkeydown?: (e: KeyboardEvent) => void;
}

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps {
  value?: string;
  options: SelectOption[];
  disabled?: boolean;
  required?: boolean;
  name?: string;
  label?: string;
  placeholder?: string;
  class?: string;
  onchange?: (e: Event) => void;
}

/**
 * Entity-schema primitives (Phase B, design-dojo data kit).
 * A schema is an ordered list of field descriptors. `DataGrid` derives its
 * columns from it and `SchemaForm` derives its inputs from it.
 */
export type SchemaFieldType = 'string' | 'number' | 'boolean' | 'datetime' | 'select';

export interface SchemaField {
  name: string;
  type: SchemaFieldType;
  description?: string;
  /** Human-friendly column/label override; defaults to a titleised `name`. */
  label?: string;
  /** Options for `select` fields. */
  options?: SelectOption[];
  required?: boolean;
  /** Hide from the grid (still available to forms) or vice-versa. */
  hidden?: boolean;
}

export interface EntitySchema {
  name?: string;
  fields: SchemaField[];
}

export type DataRow = Record<string, unknown>;

export type SortDirection = 'asc' | 'desc';

export interface DataGridProps {
  schema: EntitySchema;
  rows: DataRow[];
  /** Rows per page; 0 disables pagination. */
  pageSize?: number;
  /** Enable the per-column filter row. */
  filterable?: boolean;
  /** Enable clickable column-header sorting. */
  sortable?: boolean;
  /** Row-click handler (receives the row record). */
  onRowClick?: (row: DataRow) => void;
  class?: string;
}

/**
 * GraphView primitives (ADR-0032) — ego-centric, space-adaptive graph navigation.
 * `GraphView` is a *view* over graph data the host supplies from PluresDB; it never
 * owns persistent state and never walks the graph itself (host-mediated navigation).
 */

/** Progressive-disclosure detail level a node is granted from its box. */
export type DetailLevel = 'icon' | 'title' | 'title+keyFields' | 'full';

/** The px box a node is granted, plus the active zoom multiplier. */
export interface SpaceBudget {
  w: number;
  h: number;
  zoom: number;
}

/** The "minimum" box that governs neighbor reflow on expansion. */
export interface MinNodeSize {
  w: number;
  h: number;
}

/** A per-node affordance (drill-down / walk / custom action). */
export interface GraphNodeAction {
  id: string;
  label: string;
  icon?: string;
}

export interface GraphNode {
  id: string;
  /** entity type -> drives which schema/summary applies. */
  type?: string;
  label: string;
  /** field values, surfaced progressively by detail level. */
  fields?: Record<string, unknown>;
  /** drill-down / custom actions. */
  actions?: GraphNodeAction[];
}

export interface GraphEdge {
  id: string;
  from: string;
  to: string;
  /** relationship name. */
  label?: string;
  directed?: boolean;
}

/** What the host provides for the current focus (focus + neighbor stubs). */
export interface GraphNeighborhood {
  focusId: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface GraphViewProps {
  neighborhood: GraphNeighborhood;
  /** re-center -> host re-queries PluresDB for the new focus's neighborhood. */
  onFocusChange?: (nodeId: string) => void;
  /** request fuller detail (host may load more fields). */
  onExpand?: (nodeId: string) => void;
  onAction?: (nodeId: string, actionId: string) => void;
  /** optional override of the auto-summarizer. */
  detailFor?: (node: GraphNode, space: SpaceBudget) => DetailLevel;
  /** the minimum box that governs neighbor reflow. */
  minNodeSize?: MinNodeSize;
  /** container-space multiplier (zoom); feeds the space budget. */
  zoom?: number;
  /** render TUI token set (focus card + labeled edge list) instead of the GUI radial layout. */
  tui?: boolean;
  class?: string;
}

export type SchemaFormErrors = Record<string, string>;

export interface SchemaFormProps {
  schema: EntitySchema;
  /** Initial/edit record; omit for a blank create form. */
  value?: DataRow;
  /** Optional synchronous validation hook returning field -> message. */
  validate?: (record: DataRow) => SchemaFormErrors;
  submitLabel?: string;
  cancelLabel?: string;
  disabled?: boolean;
  /** Fired with the assembled record when the form validates + submits. */
  onsubmit?: (record: DataRow) => void;
  oncancel?: () => void;
  class?: string;
}

/**
 * Runtime-customization primitives (Phase B, ADR-0031 §6).
 * `FieldEditor` edits ONE field; `SchemaDesigner` manages a whole `EntitySchema`.
 * Both are controlled components that own NO persistent state — every edit is
 * emitted as a typed `SchemaDelta` (see `schema-delta.ts`) that the HOST persists
 * to PluresDB (C-PLURES-003) with a `.px` migration rule for existing rows.
 */
export interface FieldEditorProps {
  /** Field being edited; omit for a blank "create field" form. */
  field?: SchemaField;
  submitLabel?: string;
  cancelLabel?: string;
  disabled?: boolean;
  /** Sibling field names, used to reject duplicates (excludes `field.name`). */
  existingNames?: string[];
  /** Fired on every valid keystroke with the current draft (live preview). */
  onchange?: (field: SchemaField) => void;
  /** Fired with the field definition when the editor validates + submits. */
  onsubmit?: (field: SchemaField) => void;
  oncancel?: () => void;
  class?: string;
}

export interface SchemaDesignerProps {
  /** The schema to edit. Controlled — never mutated in place. */
  schema: EntitySchema;
  disabled?: boolean;
  /** Fired with the FULL updated schema after every accepted edit. */
  onschemachange?: (schema: EntitySchema) => void;
  /** Fired with the minimal typed delta the host should persist/migrate. */
  ondelta?: (delta: SchemaDelta) => void;
  class?: string;
}
