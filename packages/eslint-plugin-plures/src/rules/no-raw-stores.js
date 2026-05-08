/**
 * plures/no-raw-stores — Disallow raw Svelte stores and $state() runes.
 *
 * All reactive state must go through Unum (PluresDB-backed stores).
 * Raw writable(), readable(), derived(), and $state() bypass PluresDB,
 * meaning Chronos can't record mutations and Praxis can't validate them.
 *
 * This rule catches:
 * - import { writable, readable, derived } from 'svelte/store'
 * - writable(), readable(), derived() calls
 * - $state() rune usage (Svelte 5)
 * - $derived() rune usage (Svelte 5)
 *
 * Allowed exceptions:
 * - Files in allowFiles pattern (migration scaffolding)
 * - Files in stores/ that are explicitly marked as adapters
 */

const BANNED_IMPORTS = new Set(["writable", "readable", "derived"]);
const BANNED_STORE_SOURCE = "svelte/store";
const BANNED_RUNES = new Set(["$state", "$derived"]);

export default {
  meta: {
    type: "problem",
    docs: {
      description:
        "Disallow raw Svelte stores; require Unum/PluresDB-backed state",
      category: "Platform Constraints",
      recommended: true,
    },
    schema: [
      {
        type: "object",
        properties: {
          allowFiles: {
            type: "array",
            items: { type: "string" },
            description:
              "File patterns where raw stores are temporarily allowed (migration)",
          },
          allowAdapterFiles: {
            type: "array",
            items: { type: "string" },
            description:
              "Adapter files that bridge raw stores to PluresDB (e.g. plures-db-adapter.ts)",
          },
        },
        additionalProperties: false,
      },
    ],
    messages: {
      noRawStoreImport:
        "Import of '{{name}}' from 'svelte/store' is not allowed. " +
        "Use Unum/PluresDB-backed stores instead. " +
        "All state must flow through PluresDB for Chronos to record it.",
      noRawStoreCall:
        "Call to {{name}}() creates a raw Svelte store. " +
        "Use Unum/PluresDB-backed state instead.",
      noStateRune:
        "$state() bypasses PluresDB — mutations won't be recorded by Chronos. " +
        "Use Unum reactive bindings instead.",
      noDerivedRune:
        "$derived() bypasses the reactive graph — use Unum query() or computed stores instead.",
    },
  },
  create(context) {
    const options = context.options[0] || {};
    const allowFiles = options.allowFiles || [];
    const allowAdapterFiles = options.allowAdapterFiles || [
      "plures-db-adapter",
    ];

    const filename = context.getFilename?.() || context.filename || "";
    const isAllowed =
      allowFiles.some((p) => filename.includes(p)) ||
      allowAdapterFiles.some((p) => filename.includes(p));

    if (isAllowed) return {};

    return {
      // Catch: import { writable } from 'svelte/store'
      ImportDeclaration(node) {
        if (node.source.value !== BANNED_STORE_SOURCE) return;

        for (const spec of node.specifiers) {
          if (
            spec.type === "ImportSpecifier" &&
            BANNED_IMPORTS.has(spec.imported.name)
          ) {
            context.report({
              node: spec,
              messageId: "noRawStoreImport",
              data: { name: spec.imported.name },
            });
          }
        }
      },

      // Catch: writable(), readable(), derived() calls
      CallExpression(node) {
        if (
          node.callee.type === "Identifier" &&
          BANNED_IMPORTS.has(node.callee.name)
        ) {
          context.report({
            node,
            messageId: "noRawStoreCall",
            data: { name: node.callee.name },
          });
        }

        // Catch $state() and $derived() rune calls
        if (node.callee.type === "Identifier") {
          if (node.callee.name === "$state") {
            context.report({ node, messageId: "noStateRune" });
          } else if (node.callee.name === "$derived") {
            context.report({ node, messageId: "noDerivedRune" });
          }
        }
      },
    };
  },
};
