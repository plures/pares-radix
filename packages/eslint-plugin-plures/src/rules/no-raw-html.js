/**
 * plures/no-raw-html — Disallow raw HTML elements in Svelte templates.
 *
 * All UI must use design-dojo components. Raw <div>, <button>, <input>, etc.
 * are compile errors. This ensures the app is fully componentized and every
 * element participates in the design system.
 *
 * Allowed exceptions:
 * - <slot> (Svelte slots)
 * - <svelte:*> (Svelte special elements)
 * - Elements inside design-dojo package itself (allowInPackages option)
 * - Elements in files matching allowFiles pattern
 */

// HTML elements that are ALWAYS banned in pares-radix app code.
// We ban everything and only allow design-dojo components (PascalCase).
const BANNED_ELEMENTS = new Set([
  // Structure
  "div", "span", "section", "article", "aside", "header", "footer",
  "main", "nav", "figure", "figcaption", "details", "summary",
  // Text
  "p", "h1", "h2", "h3", "h4", "h5", "h6", "blockquote", "pre", "code",
  "em", "strong", "small", "mark", "del", "ins", "sub", "sup", "br", "hr",
  // Forms
  "form", "input", "textarea", "select", "option", "button", "label",
  "fieldset", "legend", "output", "datalist",
  // Tables
  "table", "thead", "tbody", "tfoot", "tr", "th", "td", "caption", "colgroup", "col",
  // Media
  "img", "video", "audio", "source", "canvas", "svg", "picture",
  // Lists
  "ul", "ol", "li", "dl", "dt", "dd",
  // Links
  "a",
  // Other
  "iframe", "dialog", "progress", "meter",
]);

// Elements that are always allowed (not HTML, or Svelte internals)
const ALWAYS_ALLOWED = new Set([
  "slot", "svelte:head", "svelte:body", "svelte:window", "svelte:document",
  "svelte:component", "svelte:self", "svelte:fragment", "svelte:element",
  "svelte:options", "svelte:boundary",
]);

export default {
  meta: {
    type: "problem",
    docs: {
      description: "Disallow raw HTML elements; require design-dojo components",
      category: "Platform Constraints",
      recommended: true,
    },
    schema: [
      {
        type: "object",
        properties: {
          allowInPackages: {
            type: "array",
            items: { type: "string" },
            description: "Package directories where raw HTML is allowed (e.g. design-dojo)",
          },
          allowFiles: {
            type: "array",
            items: { type: "string" },
            description: "File glob patterns where raw HTML is allowed",
          },
          allowElements: {
            type: "array",
            items: { type: "string" },
            description: "Specific HTML elements to allow (escape hatch)",
          },
        },
        additionalProperties: false,
      },
    ],
    messages: {
      noRawHtml:
        "Raw HTML element <{{element}}> is not allowed. Use a design-dojo component instead. " +
        "If design-dojo doesn't have an equivalent, add it to design-dojo first.",
    },
  },
  create(context) {
    const options = context.options[0] || {};
    const allowElements = new Set(options.allowElements || []);
    const allowInPackages = options.allowInPackages || ["design-dojo"];
    const allowFiles = options.allowFiles || [];

    // Check if current file is in an allowed package
    const filename = context.getFilename?.() || context.filename || "";
    const isAllowedPackage = allowInPackages.some((pkg) =>
      filename.includes(`/${pkg}/`) || filename.includes(`\\${pkg}\\`)
    );
    const isAllowedFile = allowFiles.some((pattern) =>
      filename.includes(pattern)
    );

    if (isAllowedPackage || isAllowedFile) {
      return {}; // Skip enforcement in allowed locations
    }

    return {
      // For Svelte templates parsed by svelte-eslint-parser
      SvelteElement(node) {
        // SvelteElement with kind: "html" means a raw HTML element
        if (node.kind !== "html") return;

        const name = node.name?.name || node.name;
        if (!name || typeof name !== "string") return;

        // Allow svelte: prefixed elements
        if (name.startsWith("svelte:")) return;
        if (ALWAYS_ALLOWED.has(name)) return;

        // PascalCase = component, allowed
        if (name[0] === name[0].toUpperCase()) return;

        // Allow explicitly permitted elements
        if (allowElements.has(name)) return;

        // Banned
        if (BANNED_ELEMENTS.has(name)) {
          context.report({
            node,
            messageId: "noRawHtml",
            data: { element: name },
          });
        }
      },
    };
  },
};
