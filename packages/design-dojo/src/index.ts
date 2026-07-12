/**
 * @plures/design-dojo — Local compatibility shim
 *
 * Re-exports everything from the npm package (@plures/design-dojo@0.12.0)
 * and adds 6 components that haven't been published to npm yet.
 *
 * REMOVE THIS SHIM when npm @plures/design-dojo is updated to v0.13.0+
 * with Heading, TextArea, Link, CodeBlock, Canvas2D, PluginContentArea.
 */

// Re-export everything from npm
export {
  // Primitives
  Button,   Text, Toggle, MarkdownEditor,
  // Layout
  Box, SplitPane, StatusBar, StatusBarItem, StatusBarSpacer,
  Tabs, TitleBar, ActivityBar, MenuBar, EditorTabs,
  DashboardGridItem,
  // Overlays
  Tooltip, Popover,  Toast, Menu, ContextMenu, CommandPalette,
  Wizard,
  // Data
  Table, List, ListItem, TreeView,
  // Surfaces
  Card, GlassPanel, Pane, ChatPane,
  // Feedback
  ProgressBar, Badge, Callout, EmptyState, NotificationStack,
  // Forms
  RadioGroup, FileUpload, SettingsForm,
  // Disclosure
  Accordion,
  // Icons
  NerdFont,
  // Security
  PasswordCard, VaultList, MasterPasswordPrompt,
  // App
   
  // Widgets
  StatCard,
} from '@plures/design-dojo-npm';

// Missing components — local until npm is updated
export { default as SettingsPanel } from './SettingsPanel.svelte';
export { default as Sidebar } from './Sidebar.svelte';
export { default as Input } from './Input.svelte';
export { default as Select } from './Select.svelte';
export { default as Dialog } from './Dialog.svelte';
export { default as DashboardGrid } from './DashboardGrid.svelte';
export { default as FirstRunWizard } from './FirstRunWizard.svelte';
export { default as Heading } from './Heading.svelte';
export { default as TextArea } from './TextArea.svelte';
export { default as Link } from './Link.svelte';
export { default as CodeBlock } from './CodeBlock.svelte';
export { default as Canvas2D } from './Canvas2D.svelte';
export { default as PluginContentArea } from './PluginContentArea.svelte';
export { default as DataGrid } from './DataGrid.svelte';
export { default as SchemaForm } from './SchemaForm.svelte';
export { default as FieldEditor } from './FieldEditor.svelte';
export { default as SchemaDesigner } from './SchemaDesigner.svelte';
export { applyDelta, diffField } from './schema-delta.js';

export type CommandItem = { id: string; label: string; icon?: string; action: () => void; };

export type { DashboardWidgetItem, DashboardGridProps, WizardStep, FirstRunWizardProps, SettingInputType, SettingDefinition, SettingsPanelProps, SidebarNavItem, SidebarProps, CommandPaletteProps, StatusItem, StatusBarProps, PluginContentAreaProps, SchemaFieldType, SchemaField, EntitySchema, DataRow, SortDirection, DataGridProps, SchemaFormErrors, SchemaFormProps, FieldEditorProps, SchemaDesignerProps, SchemaDelta } from './types-local.js';
