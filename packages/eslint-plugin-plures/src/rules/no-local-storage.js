/**
 * plures/no-local-storage — Disallow localStorage and sessionStorage.
 *
 * All persistent data must go through PluresDB. localStorage/sessionStorage
 * bypass the graph, meaning no Chronos recording, no Praxis validation,
 * no Hyperswarm sync, and no replay capability.
 *
 * Also catches:
 * - indexedDB direct usage
 * - document.cookie writes
 */

export default {
  meta: {
    type: "problem",
    docs: {
      description:
        "Disallow localStorage/sessionStorage/indexedDB; require PluresDB",
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
            description: "File patterns where direct storage is allowed",
          },
        },
        additionalProperties: false,
      },
    ],
    messages: {
      noLocalStorage:
        "localStorage is not allowed. Use PluresDB for all persistent data. " +
        "PluresDB gives you: Chronos recording, Praxis validation, Hyperswarm sync, and full replay.",
      noSessionStorage:
        "sessionStorage is not allowed. Use PluresDB for all state.",
      noIndexedDB:
        "Direct indexedDB usage is not allowed. Use PluresDB (which may use IndexedDB internally).",
      noCookieWrite:
        "document.cookie writes are not allowed for data storage. Use PluresDB.",
    },
  },
  create(context) {
    const options = context.options[0] || {};
    const allowFiles = options.allowFiles || [];

    const filename = context.getFilename?.() || context.filename || "";
    if (allowFiles.some((p) => filename.includes(p))) return {};

    return {
      MemberExpression(node) {
        // localStorage.setItem, localStorage.getItem, etc.
        if (node.object.type === "Identifier") {
          if (node.object.name === "localStorage") {
            context.report({ node, messageId: "noLocalStorage" });
          } else if (node.object.name === "sessionStorage") {
            context.report({ node, messageId: "noSessionStorage" });
          } else if (node.object.name === "indexedDB") {
            context.report({ node, messageId: "noIndexedDB" });
          }
        }

        // window.localStorage, window.sessionStorage
        if (
          node.object.type === "Identifier" &&
          node.object.name === "window" &&
          node.property.type === "Identifier"
        ) {
          if (node.property.name === "localStorage") {
            context.report({ node, messageId: "noLocalStorage" });
          } else if (node.property.name === "sessionStorage") {
            context.report({ node, messageId: "noSessionStorage" });
          }
        }

        // document.cookie assignment detection happens at AssignmentExpression
      },

      AssignmentExpression(node) {
        // document.cookie = '...'
        if (
          node.left.type === "MemberExpression" &&
          node.left.object.type === "Identifier" &&
          node.left.object.name === "document" &&
          node.left.property.type === "Identifier" &&
          node.left.property.name === "cookie"
        ) {
          context.report({ node, messageId: "noCookieWrite" });
        }
      },
    };
  },
};
