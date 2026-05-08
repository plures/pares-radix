/**
 * eslint-plugin-plures — Compile-time enforcement of plures platform constraints.
 *
 * Rules:
 * - plures/no-raw-html       — No raw HTML elements in Svelte; use design-dojo components
 * - plures/no-raw-stores     — No writable()/readable()/$state(); use Unum/PluresDB stores
 * - plures/no-local-storage  — No localStorage/sessionStorage; use PluresDB
 * - plures/no-manual-logging — No console.log/tracing for business events; Chronos handles it
 */

import noRawHtml from "./rules/no-raw-html.js";
import noRawStores from "./rules/no-raw-stores.js";
import noLocalStorage from "./rules/no-local-storage.js";
import noManualLogging from "./rules/no-manual-logging.js";

const plugin = {
  meta: {
    name: "eslint-plugin-plures",
    version: "0.1.0",
  },
  rules: {
    "no-raw-html": noRawHtml,
    "no-raw-stores": noRawStores,
    "no-local-storage": noLocalStorage,
    "no-manual-logging": noManualLogging,
  },
  configs: {
    recommended: {
      plugins: ["plures"],
      rules: {
        "plures/no-raw-html": "error",
        "plures/no-raw-stores": "error",
        "plures/no-local-storage": "error",
        "plures/no-manual-logging": "warn",
      },
    },
  },
};

export default plugin;
