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
  Button, Input, Select, Text, Toggle, SearchInput, MarkdownEditor,
  // Layout
  Box, SplitPane, StatusBar, StatusBarItem, StatusBarSpacer,
  Tabs, TitleBar, ActivityBar, MenuBar, EditorTabs,
  DashboardGrid, DashboardGridItem,
  // Overlays
  Tooltip, Popover, Dialog, Toast, Menu, ContextMenu, CommandPalette,
  Wizard, ConfirmDialog,
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
  FirstRunWizard, 
  // Widgets
  StatCard,
} from '@plures/design-dojo-npm';

// Missing components — local until npm is updated
export { default as Sidebar } from './Sidebar.svelte';
export { default as SettingsPanel } from './SettingsPanel.svelte';
export { default as Heading } from './Heading.svelte';
export { default as TextArea } from './TextArea.svelte';
export { default as Link } from './Link.svelte';
export { default as CodeBlock } from './CodeBlock.svelte';
export { default as Canvas2D } from './Canvas2D.svelte';
export { default as PluginContentArea } from './PluginContentArea.svelte';
