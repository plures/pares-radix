export { default as Dialog } from './Dialog.svelte';
export { default as DashboardGrid } from './DashboardGrid.svelte';
export { default as FirstRunWizard } from './FirstRunWizard.svelte';
export { default as SettingsPanel } from './SettingsPanel.svelte';
export { default as Button } from './Button.svelte';
export { default as Sidebar } from './Sidebar.svelte';
export { default as CommandPalette } from './CommandPalette.svelte';
export { default as PluginContentArea } from './PluginContentArea.svelte';
export { default as StatusBar } from './StatusBar.svelte';

// New primitives
export { default as Box } from './Box.svelte';
export { default as Text } from './Text.svelte';
export { default as Heading } from './Heading.svelte';
export { default as Input } from './Input.svelte';
export { default as TextArea } from './TextArea.svelte';
export { default as Select } from './Select.svelte';
export { default as Link } from './Link.svelte';
export { default as CodeBlock } from './CodeBlock.svelte';
export { default as List } from './List.svelte';
export { default as ListItem } from './ListItem.svelte';
export { default as Table } from './Table.svelte';

export type {
	DialogProps,
	DashboardGridProps,
	DashboardWidgetItem,
	FirstRunWizardProps,
	WizardStep,
	SettingsPanelProps,
	SettingDefinition,
	SettingInputType,
	ButtonProps,
	SidebarProps,
	SidebarNavItem,
	CommandPaletteProps,
	CommandItem,
	PluginContentAreaProps,
	StatusBarProps,
	StatusItem,
	// New primitive types
	BoxProps,
	TextProps,
	HeadingProps,
	InputProps,
	TextAreaProps,
	SelectProps,
	SelectOption,
	LinkProps,
	CodeBlockProps,
	ListProps,
	ListItemProps,
	TableProps,
} from './types.js';
