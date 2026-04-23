// pares-radix — public API
export * from './types/plugin.js';
export {
  registerPlugin,
  activateAll,
  deactivateAll,
  getAllRoutes,
  getAllNavItems,
  getAllSettings,
  getAllDashboardWidgets,
  getAllHelpSections,
  getAllOnboardingSteps,
  getAllInferenceRules,
  getAllExpectations,
  getAllConstraints,
  getPlugin,
  getPluginIds,
  isPluginActive,
  exportAllPluginData,
  importAllPluginData,
  getActivePluginManifests,
} from './platform/plugin-loader.js';
export {
  createInferenceEngine,
  needsUserConfirmation,
  isAutoConfirmed,
} from './platform/inference-engine.js';
export { createLLMAPI, resetTokenBudget, getTokensUsed } from './platform/llm.js';
export {
  builtinUxExpectations,
  checkDataRequirements,
  validateUxExpectations,
} from './praxis/ux-contracts.js';
export {
  shellModule,
  defineContract,
  validateModule,
  scanRules,
} from './praxis/shell.js';
export { agensModule } from './praxis/agens.js';
export {
  designModule,
  buildSchemaRegistry,
} from './praxis/design.js';
export type {
  DesignSchema,
  DesignDraft,
  SchemaKind,
} from './praxis/design.js';
export {
  registerForHotReload,
  getLiveModules,
  applySchemaChange,
  recordDecision,
  getDecisionLedger,
} from './praxis/hot-reload.js';
export type { DesignDecision } from './praxis/hot-reload.js';
export type {
  PraxisFact,
  PraxisEvent,
  ContractExample,
  ContractInvariant,
  Contract,
  PraxisContext,
  PraxisRule,
  PraxisSystemState,
  PraxisConstraint,
  PraxisGate,
  PraxisModule,
  ValidationResult,
} from './types/praxis.js';
export {
  EXPORT_FORMAT_VERSION,
  createExport,
  validateImport,
} from './platform/data-transfer.js';
export type {
  PluginManifestEntry,
  RadixExportMeta,
  RadixExport,
} from './platform/data-transfer.js';
export {
  createPluresDBAdapter,
  localStorageGraph,
  setSharedGraph,
  getSharedGraph,
  setSharedAdapter,
  getSharedAdapter,
  FACT_PREFIX,
  PLUGIN_DATA_PREFIX,
  SETTING_PREFIX,
} from './stores/plures-db-adapter.js';
export type {
  PluresDBGraph,
  PluresDBAdapter,
  PluresDBAdapterOptions,
} from './stores/plures-db-adapter.js';
