import type { SvelteComponent, Snippet } from 'svelte';

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
