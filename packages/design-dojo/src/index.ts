export { default as Dialog } from './Dialog.svelte';
export { default as DashboardGrid } from './DashboardGrid.svelte';
export { default as FirstRunWizard } from './FirstRunWizard.svelte';
export { default as SettingsPanel } from './SettingsPanel.svelte';
export { default as Button } from './Button.svelte';
export { default as Sidebar } from './Sidebar.svelte';
export { default as CommandPalette } from './CommandPalette.svelte';
export { default as PluginContentArea } from './PluginContentArea.svelte';
export { default as StatusBar } from './StatusBar.svelte';

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
} from './types.js';
