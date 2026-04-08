import type { Snippet, SvelteComponent } from 'svelte';

/** Props accepted by the Dialog component. */
export interface DialogProps {
	open: boolean;
	title: string;
	message: string;
	confirmLabel?: string;
	cancelLabel?: string;
	onConfirm: () => void;
	onCancel: () => void;
}

/** A single widget entry for the DashboardGrid. */
export interface DashboardWidgetItem {
	/** Widget ID */
	id: string;
	/** Display title */
	title: string;
	/** Async component loader */
	component: () => Promise<{ default: typeof SvelteComponent }>;
	/** Grid column span (1–4) */
	colspan?: number;
}

/** A single step in the FirstRunWizard. */
export interface WizardStep {
	/** Step title */
	title: string;
	/** Description */
	description: string;
	/** Emoji or icon */
	icon: string;
	/** URL to navigate to when user clicks the action */
	href: string;
	/** Action button label */
	actionLabel: string;
	/** Check if this step is complete */
	isComplete: () => boolean | Promise<boolean>;
	/** Step titles that must complete first */
	after?: string[];
}

/** Supported setting input types. */
export type SettingInputType = 'toggle' | 'select' | 'text' | 'number' | 'password' | 'color';

/** A single setting definition for the SettingsPanel. */
export interface SettingDefinition {
	/** Unique key (namespaced: "plugin-id.setting-name") */
	key: string;
	/** Setting type */
	type: SettingInputType;
	/** Display label */
	label: string;
	/** Description text */
	description?: string;
	/** Default value */
	default: unknown;
	/** Options for 'select' type */
	options?: { value: string; label: string }[];
	/** Group name for visual organisation */
	group?: string;
}

/** Props accepted by the SettingsPanel component. */
export interface SettingsPanelProps {
	groupName: string;
	settings: SettingDefinition[];
	getValue: (key: string) => unknown;
	setValue: (key: string, value: unknown) => void;
}

/** Props accepted by the FirstRunWizard component. */
export interface FirstRunWizardProps {
	steps: WizardStep[];
	isComplete: (stepTitle: string) => boolean;
	markComplete: (stepTitle: string) => void;
}

/** Props accepted by the DashboardGrid component. */
export interface DashboardGridProps {
	widgets: DashboardWidgetItem[];
}

/** Props accepted by the Button component. */
export interface ButtonProps {
	variant?: 'primary' | 'secondary';
	disabled?: boolean;
	onclick?: (e: MouseEvent) => void;
}

/** A navigation item for the Sidebar. */
export interface SidebarNavItem {
	/** URL path */
	href: string;
	/** Display label */
	label: string;
	/** Emoji or icon character */
	icon?: string;
	/** Badge count (e.g. unread items) */
	badge?: number;
}

/** Props accepted by the Sidebar component. */
export interface SidebarProps {
	/** Navigation items to render */
	items: SidebarNavItem[];
	/** Current browser pathname for active-link highlighting */
	currentPath: string;
	/** Whether the sidebar is collapsed to icon-only mode */
	collapsed?: boolean;
	/** Callback fired when the user requests a collapse toggle */
	onToggle?: () => void;
}

/** A single command entry for the CommandPalette. */
export interface CommandItem {
	/** Unique identifier */
	id: string;
	/** Display label */
	label: string;
	/** Emoji or icon */
	icon?: string;
	/** Invoked when the user selects this command */
	action: () => void;
}

/** Props accepted by the CommandPalette component. */
export interface CommandPaletteProps {
	/** Whether the palette is open */
	open?: boolean;
	/** Available commands */
	commands?: CommandItem[];
	/** Called when the palette should close */
	onClose?: () => void;
}

/** A single entry in the StatusBar. */
export interface StatusItem {
	/** Short label prefix (e.g. "Theme") */
	label: string;
	/** Current value (e.g. "dark") */
	value: string;
}

/** Props accepted by the StatusBar component. */
export interface StatusBarProps {
	/** Items to display; first set is left-aligned, last is right-aligned */
	items?: StatusItem[];
}

/** Props accepted by the PluginContentArea component. */
export interface PluginContentAreaProps {
	/** Current theme value — controls the toggle icon */
	theme?: string;
	/** Called when the user requests a theme toggle */
	onThemeToggle?: () => void;
	/** Called when the user requests a sidebar toggle */
	onSidebarToggle?: () => void;
	/** Called when the user requests the command palette */
	onCommandPaletteOpen?: () => void;
	/** Status bar items (rendered at the bottom of the content area) */
	statusItems?: StatusItem[];
	/** Page content */
	children: Snippet;
}
