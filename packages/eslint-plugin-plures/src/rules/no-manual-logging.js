/**
 * plures/no-manual-logging — Disallow manual logging for business events.
 *
 * Chronos handles all logging automatically through contracts and procedures.
 * Manual console.log/warn/error and tracing calls for business events are
 * an anti-pattern — they bypass the contract system, don't participate in
 * the rolling buffer, and can't be replayed.
 *
 * Allowed:
 * - console.* in test files
 * - console.* in scripts/ directory
 * - Explicitly tagged debug lines (// eslint-disable-next-line plures/no-manual-logging)
 *
 * Note: This is a "warn" by default, not "error", because migration is gradual.
 * Once all logging flows through contracts, upgrade to "error".
 */

const BANNED_METHODS = new Set(["log", "warn", "error", "info", "debug", "trace"]);

export default {
  meta: {
    type: "suggestion",
    docs: {
      description:
        "Disallow manual console/tracing calls; Chronos contracts handle logging",
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
            description: "File patterns where manual logging is allowed",
          },
          allowMethods: {
            type: "array",
            items: { type: "string" },
            description: "Specific console methods to allow (e.g. ['error'] for critical paths)",
          },
        },
        additionalProperties: false,
      },
    ],
    messages: {
      noConsoleLog:
        "console.{{method}}() bypasses Chronos contracts. " +
        "Business events should be logged automatically through praxis contracts. " +
        "If this is a temporary debug line, disable the rule inline.",
    },
  },
  create(context) {
    const options = context.options[0] || {};
    const allowFiles = options.allowFiles || ["test", "spec", "scripts/"];
    const allowMethods = new Set(options.allowMethods || []);

    const filename = context.getFilename?.() || context.filename || "";
    if (allowFiles.some((p) => filename.includes(p))) return {};

    return {
      CallExpression(node) {
        if (
          node.callee.type === "MemberExpression" &&
          node.callee.object.type === "Identifier" &&
          node.callee.object.name === "console" &&
          node.callee.property.type === "Identifier" &&
          BANNED_METHODS.has(node.callee.property.name) &&
          !allowMethods.has(node.callee.property.name)
        ) {
          context.report({
            node,
            messageId: "noConsoleLog",
            data: { method: node.callee.property.name },
          });
        }
      },
    };
  },
};
