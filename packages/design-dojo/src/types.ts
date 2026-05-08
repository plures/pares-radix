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
	variant?: 'primary' | 'secondary' | 'danger' | 'ghost';
	disabled?: boolean;
	onclick?: (e: MouseEvent) => void;
	/** Button type */
	type?: 'button' | 'submit' | 'reset';
	/** Additional CSS class */
	class?: string;
	/** Inline style string */
	style?: string;
	/** Passthrough data/aria attributes */
	[key: `data-${string}`]: string | undefined;
	[key: `aria-${string}`]: string | undefined;
	title?: string;
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

// ── New Primitives ──────────────────────────────────────────────────────────

/** Props for Box — the universal layout container. Replaces div/section/article/aside/main/nav/header/footer. */
export interface BoxProps {
	/** HTML element to render (default: 'div') */
	as?: string;
	/** CSS padding value */
	padding?: string;
	/** CSS gap value */
	gap?: string;
	/** Flex direction: 'row' | 'column' (default: 'column') */
	direction?: 'row' | 'column';
	/** CSS align-items */
	align?: string;
	/** CSS justify-content */
	justify?: string;
	/** Flex wrap */
	wrap?: boolean;
	/** Additional CSS class */
	class?: string;
	/** Inline style string */
	style?: string;
	/** Click handler (makes the box interactive) */
	onclick?: (e: MouseEvent) => void;
	/** Passthrough HTML attributes (aria-*, data-*, role, etc.) */
	[key: `aria-${string}`]: string | undefined;
	[key: `data-${string}`]: string | undefined;
	role?: string;
	tabindex?: number;
}

/** Props for Text — replaces span/p. */
export interface TextProps {
	/** HTML element to render (default: 'span') */
	as?: 'span' | 'p' | 'em' | 'strong' | 'small' | 'mark' | 'kbd' | 'code' | 'div' | 'label';
	/** CSS font-size */
	size?: string;
	/** CSS font-weight */
	weight?: string;
	/** CSS color */
	color?: string;
	/** Inline style string */
	style?: string;
	/** Truncate with ellipsis */
	truncate?: boolean;
	/** Additional CSS class */
	class?: string;
	/** Passthrough HTML attributes */
	[key: `aria-${string}`]: string | boolean | undefined;
	[key: `data-${string}`]: string | undefined;
	title?: string;
	role?: string;
}

/** Props for Heading — replaces h1-h6. */
export interface HeadingProps {
	/** Heading level 1-6 (default: 2) */
	level?: 1 | 2 | 3 | 4 | 5 | 6;
	/** Additional CSS class */
	class?: string;
}

/** Props for Input — replaces input/label. */
export interface InputProps {
	/** Input type */
	type?: 'text' | 'number' | 'password' | 'email' | 'url' | 'search' | 'tel' | 'date' | 'color' | 'checkbox' | 'range' | 'file' | (string & {});
	/** Bound value */
	value?: string | number;
	/** Placeholder text */
	placeholder?: string;
	/** Disabled state */
	disabled?: boolean;
	/** Required field */
	required?: boolean;
	/** Checked state (for checkbox) */
	checked?: boolean;
	/** Input name/id */
	name?: string;
	/** Label text (renders above input) */
	label?: string;
	/** Error message (renders below input) */
	error?: string;
	/** Additional CSS class */
	class?: string;
	/** Input event handler */
	oninput?: (e: Event) => void;
	/** Change event handler */
	onchange?: (e: Event) => void;
	/** Submit event handler */
	onsubmit?: (e: Event) => void;
	/** Passthrough */
	[key: `data-${string}`]: string | undefined;
	[key: `aria-${string}`]: string | undefined;
	min?: number | string;
	max?: number | string;
	step?: number | string;
	autocomplete?: string;
}

/** Props for TextArea — replaces textarea. */
export interface TextAreaProps {
	/** Bound value */
	value?: string;
	/** Placeholder */
	placeholder?: string;
	/** Disabled state */
	disabled?: boolean;
	/** Required */
	required?: boolean;
	/** Number of visible rows */
	rows?: number;
	/** Name/id */
	name?: string;
	/** Label */
	label?: string;
	/** Error message */
	error?: string;
	/** Additional CSS class */
	class?: string;
	/** Input handler */
	oninput?: (e: Event) => void;
	/** Keydown handler */
	onkeydown?: (e: KeyboardEvent) => void;
}

/** Option for Select component. */
export interface SelectOption {
	value: string;
	label: string;
}

/** Props for Select — replaces select/option. */
export interface SelectProps {
	/** Bound value */
	value?: string;
	/** Available options */
	options: SelectOption[];
	/** Disabled state */
	disabled?: boolean;
	/** Required */
	required?: boolean;
	/** Name/id */
	name?: string;
	/** Label */
	label?: string;
	/** Placeholder (disabled first option) */
	placeholder?: string;
	/** Additional CSS class */
	class?: string;
	/** Change handler */
	onchange?: (e: Event) => void;
}

/** Props for Link — replaces a. */
export interface LinkProps {
	/** URL */
	href: string;
	/** Open in new tab */
	external?: boolean;
	/** Additional CSS class */
	class?: string;
}

/** Props for CodeBlock — replaces pre/code. */
export interface CodeBlockProps {
	/** Code content (alternative to children) */
	code?: string;
	/** Language hint */
	language?: string;
	/** Additional CSS class */
	class?: string;
}

/** Props for List — replaces ul/ol. */
export interface ListProps {
	/** Ordered list (ol) vs unordered (ul) */
	ordered?: boolean;
	/** Additional CSS class */
	class?: string;
}

/** Props for ListItem — replaces li. */
export interface ListItemProps {
	/** Additional CSS class */
	class?: string;
}

/** Props for Table — replaces table/thead/tbody/tr/th/td. */
export interface TableProps {
	/** Additional CSS class */
	class?: string;
}
