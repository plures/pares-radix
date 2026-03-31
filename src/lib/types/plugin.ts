/**
 * pares-radix Plugin API
 *
 * This is the contract between the radix runtime and domain plugins.
 * Every plugin implements RadixPlugin. The runtime handles everything else.
 */

import type { SvelteComponent } from 'svelte';

// ─── Plugin Manifest ────────────────────────────────────────────────────────

export interface RadixPlugin {
  /** Unique plugin identifier (kebab-case) */
  id: string;
  /** Human-readable name */
  name: string;
  /** SemVer version */
  version: string;
  /** Emoji or icon path */
  icon: string;
  /** Short description */
  description: string;
  /** Plugin IDs this depends on (loaded first) */
  dependencies?: string[];

  // ── UI Integration ──────────────────────────────────────────────────────

  /** Routes this plugin mounts */
  routes: PluginRoute[];
  /** Sidebar navigation items */
  navItems: NavItem[];
  /** Settings this plugin exposes in the unified settings page */
  settings: PluginSetting[];
  /** Dashboard widgets for the home page */
  dashboardWidgets?: DashboardWidget[];
  /** Help content sections */
  helpSections?: HelpSection[];
  /** Onboarding steps (ordered by priority) */
  onboardingSteps?: OnboardingStep[];

  // ── Praxis Integration ──────────────────────────────────────────────────

  /** Business and UX expectations */
  expectations?: Expectation[];
  /** Inference rules with confidence scoring */
  rules?: InferenceRule[];
  /** Validation constraints */
  constraints?: Constraint[];

  // ── Lifecycle ───────────────────────────────────────────────────────────

  /** Called when plugin is activated. Receives platform context. */
  onActivate?(ctx: PluginContext): Promise<void>;
  /** Called when plugin is deactivated. Clean up resources. */
  onDeactivate?(): Promise<void>;
  /** Called when platform imports data. Plugin receives its slice. */
  onDataImport?(data: unknown): Promise<void>;
  /** Called when platform exports data. Plugin returns its slice. */
  onDataExport?(): Promise<unknown>;
}

// ─── UI Types ───────────────────────────────────────────────────────────────

export interface PluginRoute {
  /** URL path (relative to plugin, e.g. "/" becomes "/financial-advisor/") */
  path: string;
  /** Svelte component loader */
  component: () => Promise<{ default: typeof SvelteComponent }>;
  /** Page title */
  title?: string;
  /** Data prerequisites — page shows empty state if unmet */
  requires?: DataRequirement[];
}

export interface NavItem {
  /** URL path */
  href: string;
  /** Display label */
  label: string;
  /** Emoji or icon */
  icon: string;
  /** Sub-items for nested navigation */
  children?: NavItem[];
  /** Badge count (e.g., unread items) */
  badge?: () => number;
}

export type SettingType = 'toggle' | 'select' | 'text' | 'number' | 'password' | 'color';

export interface PluginSetting {
  /** Unique key (namespaced: "plugin-id.setting-name") */
  key: string;
  /** Setting type */
  type: SettingType;
  /** Display label */
  label: string;
  /** Description text */
  description?: string;
  /** Default value */
  default: unknown;
  /** Options for 'select' type */
  options?: { value: string; label: string }[];
  /** Group name for visual organization */
  group?: string;
}

export interface DashboardWidget {
  /** Widget ID */
  id: string;
  /** Display title */
  title: string;
  /** Widget component */
  component: () => Promise<{ default: typeof SvelteComponent }>;
  /** Grid column span (1-4) */
  colspan?: number;
  /** Sort priority (lower = first) */
  priority?: number;
}

export interface HelpSection {
  /** Section title */
  title: string;
  /** Emoji or icon */
  icon: string;
  /** Markdown content or component */
  content: string | (() => Promise<{ default: typeof SvelteComponent }>);
  /** Sort priority */
  priority?: number;
  /** Links to relevant plugin pages shown as "See also" */
  links?: { label: string; href: string }[];
}

export interface OnboardingStep {
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
  /** Plugin IDs whose steps must complete first */
  after?: string[];
}

export interface DataRequirement {
  /** What data is needed (e.g., "accounts", "transactions") */
  type: string;
  /** Minimum count needed */
  minCount?: number;
  /** Message to show in empty state */
  emptyMessage: string;
  /** URL to navigate to for fulfillment */
  fulfillHref: string;
  /** Action label for empty state button */
  fulfillLabel: string;
}

// ─── Praxis Types ───────────────────────────────────────────────────────────

export type ExpectationDomain = 'business' | 'security' | 'performance' | 'ux' | 'inference';
export type ExpectationSeverity = 'error' | 'warning' | 'info';

export interface Expectation {
  /** Unique expectation ID */
  id: string;
  /** Domain this expectation belongs to */
  domain: ExpectationDomain;
  /** Human-readable description */
  description: string;
  /** How serious a violation is */
  severity: ExpectationSeverity;
  /** Validation function — receives current state, returns pass/fail */
  validate: (state: unknown) => boolean | Promise<boolean>;
}

export interface InferenceRule {
  /** Unique rule ID */
  id: string;
  /** Human-readable name */
  name: string;
  /** What this rule infers */
  description: string;
  /** Source data types this rule applies to */
  appliesTo: string[];
  /** Base confidence when this rule fires (0.0-1.0) */
  baseConfidence: number;
  /** Execute the rule against input data */
  evaluate(input: InferenceInput): InferenceResult | null;
}

export interface InferenceInput {
  /** The record being evaluated */
  record: Record<string, unknown>;
  /** Historical records of the same type */
  history: Record<string, unknown>[];
  /** Previous inferences for this record */
  priorInferences: Inference[];
  /** All confirmed inferences (ground truth) */
  confirmedInferences: Inference[];
}

export interface InferenceResult {
  /** What field was inferred */
  field: string;
  /** The inferred value */
  value: unknown;
  /** Confidence (0.0-1.0) */
  confidence: number;
  /** How the rule reached this conclusion */
  reasoning: string;
}

export interface Inference {
  id: string;
  sourceId: string;
  sourceType: string;
  field: string;
  value: unknown;
  confidence: number;
  strategy: string;
  decisionChain: DecisionEntry[];
  confirmed: boolean;
  confirmedBy?: 'user' | 'auto' | 'llm';
  createdAt: string;
  updatedAt: string;
}

export interface DecisionEntry {
  ruleId: string;
  input: Record<string, unknown>;
  output: unknown;
  confidenceDelta: number;
  reasoning: string;
  timestamp: string;
}

export interface Constraint {
  /** Unique constraint ID */
  id: string;
  /** Human-readable description */
  description: string;
  /** Validation function */
  validate: (data: unknown) => boolean | Promise<boolean>;
  /** Error message on violation */
  message: string;
}

// ─── Platform Context ───────────────────────────────────────────────────────

export interface PluginContext {
  /** Platform settings store */
  settings: SettingsAPI;
  /** PluresDB data access */
  data: DataAPI;
  /** LLM integration */
  llm: LLMAPI;
  /** Inference engine */
  inference: InferenceAPI;
  /** Navigation control */
  navigation: NavigationAPI;
  /** Notification/toast system */
  notify: NotifyAPI;
}

export interface SettingsAPI {
  get<T = unknown>(key: string): T | undefined;
  set(key: string, value: unknown): void;
  subscribe(key: string, callback: (value: unknown) => void): () => void;
}

export interface DataAPI {
  /** Get a PluresDB collection by name (namespaced to plugin) */
  collection(name: string): CollectionAPI;
}

export interface CollectionAPI {
  get(id: string): Promise<unknown>;
  put(id: string, data: unknown): Promise<void>;
  delete(id: string): Promise<void>;
  query(filter?: Record<string, unknown>): Promise<unknown[]>;
  count(): Promise<number>;
}

export interface LLMAPI {
  /** Check if an LLM provider is configured */
  available(): boolean;
  /** Send a prompt with assembled context */
  complete(prompt: string, context?: Record<string, unknown>): Promise<string>;
  /** Get remaining token budget for this session */
  remainingBudget(): number;
}

export interface InferenceAPI {
  /** Run all applicable rules against a record */
  infer(sourceType: string, record: Record<string, unknown>): Promise<Inference[]>;
  /** Get inferences for a specific source record */
  getInferences(sourceId: string): Promise<Inference[]>;
  /** Confirm or reject an inference (user feedback) */
  confirm(inferenceId: string, confirmed: boolean): Promise<void>;
  /** Get the decision ledger for an inference */
  getDecisionChain(inferenceId: string): Promise<DecisionEntry[]>;
}

export interface NavigationAPI {
  /** Navigate to a URL */
  goto(href: string): void;
  /** Register a breadcrumb */
  setBreadcrumbs(crumbs: { label: string; href?: string }[]): void;
}

export interface NotifyAPI {
  success(message: string): void;
  info(message: string): void;
  warning(message: string): void;
  error(message: string): void;
}
