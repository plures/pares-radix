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

// Infra visualisation family — representational shapes for physical systems
// (servers in racks, datacenters as buildings, regions as territories).
// Composed from the design-dojo SVG primitive layer; token-compliant.
export { default as StatusBeacon } from './infra/StatusBeacon.svelte';
export { default as BeaconBadge } from './infra/BeaconBadge.svelte';
export { default as ServerRack } from './infra/ServerRack.svelte';
export { default as DatacenterBuilding } from './infra/DatacenterBuilding.svelte';
export { default as RegionMap } from './infra/RegionMap.svelte';
export { default as PluginModule } from './infra/PluginModule.svelte';

// Interactive pane primitives (Wt-prefixed to avoid collision with the
// non-interactive npm SplitPane/Pane/Tabs exports above). Thin Svelte 5
// adapters over the framework-free logic in src/lib/panes/.
export { default as WtSplitPane } from './panes/WtSplitPane.svelte';
export { default as WtPane } from './panes/WtPane.svelte';
export { default as WtPaneTabs } from './panes/WtPaneTabs.svelte';
export type {
	Orientation as WtOrientation,
	TabDescriptor as WtTabDescriptor,
	DragItem as WtDragItem,
	DropTarget as WtDropTarget,
	DndSession as WtDndSession,
	MoveCommand as WtMoveCommand
} from './panes/types.js';

export type CommandItem = { id: string; label: string; icon?: string; action: () => void; };

export type { BeaconStatus, RackUnit, RegionTone, DashboardWidgetItem, DashboardGridProps, WizardStep, FirstRunWizardProps, SettingInputType, SettingDefinition, SettingsPanelProps, SidebarNavItem, SidebarProps, CommandPaletteProps, StatusItem, StatusBarProps, PluginContentAreaProps } from './types-local.js';
