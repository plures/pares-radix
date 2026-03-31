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
} from './platform/plugin-loader.js';
export {
  createInferenceEngine,
  needsUserConfirmation,
  isAutoConfirmed,
} from './platform/inference-engine.js';
export {
  builtinUxExpectations,
  checkDataRequirements,
  validateUxExpectations,
} from './praxis/ux-contracts.js';
