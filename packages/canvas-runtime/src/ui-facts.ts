/**
 * ui-facts.ts — Structured UI fact extraction for Praxis enforcement.
 *
 * The Praxis constraint evaluator (`simpleEval`) only understands flat boolean
 * logic over a context object: `context.<path> === ...`, `&&`, `||`, `!`,
 * comparisons. It cannot walk a tree, call `.includes()`, or run functions.
 *
 * Therefore, for UI best practices to be *enforced* (not merely documented as
 * `[parser-skip]` prose), we must reduce the canvas tree into a flat set of
 * structured facts that the constraints read. This module is that reducer.
 *
 * The contract: every `praxis/ui/*.px` constraint reads a `context.ui.*` fact
 * that THIS file is responsible for producing. If you add a constraint, add the
 * fact here. If you add a fact, it is testable in isolation. No constraint may
 * depend on a fact that this extractor does not emit — that is the invariant
 * that keeps the library honest (no rules that silently never fire).
 *
 * Facts are intentionally pre-aggregated (counts, booleans, worst-case numbers)
 * so the constraint expressions stay within the evaluator's flat surface.
 */

import { resolveComponent, getRegistry } from './registry.js';
import { kindForComponent } from './ui-schema.js';
import { THEME_TOKENS, THEME_SURFACE, type ThemeMode } from './ui-practices.js';
import { meetsContrast, parseHexColor, WCAG_AA_NORMAL } from './ui-contrast.js';

// ── Canvas node shape (mirrors CanvasRenderer / canvas-plugin) ───────────────

export interface CanvasNodeLike {
  id: string;
  type: string;
  props?: Record<string, unknown>;
  bindings?: Record<string, unknown>;
  /** Responsive intent map (attribute → breakpoint → value). See format.ts CanvasNode. */
  responsive?: Record<string, Record<string, unknown>>;
  /**
   * Semantic colour token (e.g. 'fg', 'muted', 'accent'). When set, the theme
   * resolve practices (ui-theme.px) map it to a concrete `color` for the active
   * theme mode. Authored intent; never written by the resolver.
   */
  themeToken?: string;
  children?: CanvasNodeLike[];
  /** Not read by the extractor; widened to accept any CanvasNode `visible` shape. */
  visible?: unknown;
}

// ── Fact context shape ───────────────────────────────────────────────────────

/**
 * The flat fact context handed to Praxis. Every field here is referenced by at
 * least one constraint in `praxis/ui/*.px`. Booleans default to the
 * "compliant" value when the relevant component is absent, so an empty/partial
 * tree never produces false violations.
 */
export interface UiFacts {
  /** Total node count (cheap complexity signal). */
  nodeCount: number;

  // ── Forms & inputs (accessibility + UX) ──
  /** Number of input-like nodes (Input, TextArea, Select). */
  inputCount: number;
  /** Number of input-like nodes missing a non-empty `label`. */
  inputsMissingLabel: number;
  /** True when every input has a label (compliant default). */
  allInputsLabeled: boolean;

  // ── Buttons & affordances ──
  buttonCount: number;
  /** Buttons whose `label` is empty/missing (no accessible name). */
  buttonsMissingLabel: number;
  allButtonsLabeled: boolean;
  /** Count of destructive-variant buttons (variant === 'danger'). */
  dangerButtonCount: number;
  /** Danger buttons not paired with a confirmation Dialog in the tree. */
  dangerButtonsWithoutConfirm: number;

  // ── Links ──
  linkCount: number;
  /** External links (external === true) — should signal new-tab/rel safety. */
  externalLinkCount: number;
  /** External links missing an accessible text child (empty link). */
  externalLinksMissingText: number;

  // ── Headings & hierarchy ──
  headingCount: number;
  /** True when the document has at least one top-level heading (level 1 or 2). */
  hasTopLevelHeading: boolean;
  /** True when heading levels are ever skipped in document order (e.g. 2 → 4). */
  headingsSkipLevel: boolean;
  /** Number of distinct level-1 headings (more than one is an a11y smell). */
  h1Count: number;

  // ── Feedback & state ──
  dialogCount: number;
  /** Dialogs missing onConfirm/onCancel handlers (dead modal). */
  dialogsMissingHandlers: number;

  // ── Images / media (alt text) ──
  imageCount: number;
  imagesMissingAlt: number;
  allImagesHaveAlt: boolean;

  // ── Colour contrast (WCAG AA) ──
  /**
   * Whether the contrast check actually ran. It runs ONLY when a theme mode is
   * supplied to extractUiFacts (so the active surface is known). When false the
   * contrast constraint stays inert — the honest "we can't know the surface yet"
   * state, NOT a silent pass. (extractUiFacts default: no mode → false.)
   */
  contrastChecked: boolean;
  /**
   * Number of CHECKABLE text nodes whose colour fails WCAG AA (< 4.5) against
   * the active mode's THEME_SURFACE background. A text node is checkable only if
   * it has a themeToken in THEME_TOKENS OR a hex `props.color`; unknown/absent
   * colours are not guessed and not counted. Always 0 when contrastChecked is
   * false.
   */
  lowContrastTextCount: number;

  // ── Unknown components (registry coverage) ──
  /** Nodes whose `type` is not in the component registry. */
  unknownComponentCount: number;
  /**
   * Number of components currently registered. When 0, the registry hasn't
   * been populated, so `unknownComponentCount` is meaningless and the
   * unknown-component constraint must not fire (guarded via `registrySize > 0`).
   */
  registrySize: number;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const INPUT_TYPES = new Set(['Input', 'TextArea', 'Select']);

function isNonEmptyString(v: unknown): boolean {
  return typeof v === 'string' && v.trim().length > 0;
}

/**
 * A prop is "provided" if it's a non-empty literal OR bound to a PluresDB key
 * (bindings make the value dynamic but present). Accessibility names satisfied
 * by a binding are still satisfied.
 */
function propProvided(node: CanvasNodeLike, prop: string): boolean {
  const literal = node.props?.[prop];
  if (isNonEmptyString(literal)) return true;
  if (typeof literal === 'boolean' || typeof literal === 'number') return true;
  // A binding (PluresDB key reference) for this prop counts as "provided":
  // the value is dynamic but present. A CanvasBinding is an object/string, so we
  // test for presence (non-null), not string-shape.
  if (node.bindings != null && bindingPresent(node.bindings[prop])) return true;
  return false;
}

/** True when a binding entry exists and is non-null (object or non-empty string). */
function bindingPresent(b: unknown): boolean {
  if (b == null) return false;
  if (typeof b === 'string') return b.trim().length > 0;
  return true; // object/array binding descriptor — present
}

function hasTextChild(node: CanvasNodeLike): boolean {
  if (isNonEmptyString(node.props?.children)) return true;
  if (isNonEmptyString((node.props as Record<string, unknown> | undefined)?.text)) return true;
  return Array.isArray(node.children) && node.children.length > 0;
}

function walk(node: CanvasNodeLike, visit: (n: CanvasNodeLike) => void): void {
  visit(node);
  if (Array.isArray(node.children)) {
    for (const child of node.children) walk(child, visit);
  }
}

/**
 * Does the subtree rooted at `root` contain a Dialog? Used to check that
 * destructive actions are paired with a confirmation surface somewhere in the
 * same app tree. (Heuristic: app-level pairing, not per-button wiring, since
 * the flat evaluator can't express per-button relationships.)
 */
function treeHasDialog(root: CanvasNodeLike): boolean {
  let found = false;
  walk(root, (n) => {
    if (n.type === 'Dialog') found = true;
  });
  return found;
}

/**
 * Is this node a TEXT-kind node (typography / colour)? Resolved via the registry
 * (category → schemaKind, e.g. Text/Heading → 'text'). Unregistered types are
 * NOT assumed to be text — honest: we only contrast-check nodes we can classify.
 */
function isTextKind(node: CanvasNodeLike): boolean {
  const meta = resolveComponent(node.type);
  if (!meta) return false;
  return kindForComponent(meta.schemaKind, meta.category) === 'text';
}

/**
 * The concrete foreground colour to contrast-check for a text node, or null when
 * the node is NOT checkable (so it is neither counted nor guessed).
 *
 * Checkable iff EITHER:
 *   - props.color is a parseable hex string (explicit author colour), which WINS
 *     (mirrors resolve precedence: an explicit literal colour beats a token); OR
 *   - themeToken names a colour in THEME_TOKENS → THEME_TOKENS[token][mode].
 * Anything else (no color/token, a non-hex colour like 'red' or a CSS var, an
 * unknown token) → null (unknown → not checked).
 */
function checkableTextColor(node: CanvasNodeLike, mode: ThemeMode): string | null {
  const explicit = node.props?.color;
  if (typeof explicit === 'string' && parseHexColor(explicit)) return explicit;
  const token = node.themeToken;
  if (typeof token === 'string' && Object.prototype.hasOwnProperty.call(THEME_TOKENS, token)) {
    return THEME_TOKENS[token][mode];
  }
  return null;
}

// ── Extractor ────────────────────────────────────────────────────────────────

/**
 * Reduce a canvas tree to flat UI facts for Praxis evaluation.
 *
 * @param root The root canvas node (or the canvas document's `tree`).
 * @param opts Optional inputs that change what can be checked. `themeMode`
 *   supplies the ACTIVE theme mode so the contrast check knows which
 *   THEME_SURFACE background to use. Omit it (the default) to keep every
 *   existing `(root)`-only call site working: with no mode the contrast check is
 *   skipped (contrastChecked=false, lowContrastTextCount=0) — honest, since the
 *   surface is unknown.
 * @returns A `UiFacts` object suitable for `{ context: { ui: facts } }`.
 */
export function extractUiFacts(
  root: CanvasNodeLike | null | undefined,
  opts: { themeMode?: ThemeMode } = {},
): UiFacts {
  const themeMode = opts.themeMode;
  const facts: UiFacts = {
    nodeCount: 0,
    inputCount: 0,
    inputsMissingLabel: 0,
    allInputsLabeled: true,
    buttonCount: 0,
    buttonsMissingLabel: 0,
    allButtonsLabeled: true,
    dangerButtonCount: 0,
    dangerButtonsWithoutConfirm: 0,
    linkCount: 0,
    externalLinkCount: 0,
    externalLinksMissingText: 0,
    headingCount: 0,
    hasTopLevelHeading: false,
    headingsSkipLevel: false,
    h1Count: 0,
    dialogCount: 0,
    dialogsMissingHandlers: 0,
    imageCount: 0,
    imagesMissingAlt: 0,
    allImagesHaveAlt: true,
    contrastChecked: themeMode !== undefined,
    lowContrastTextCount: 0,
    unknownComponentCount: 0,
    registrySize: getRegistry().size,
  };

  if (!root) return facts;

  const hasDialogSomewhere = treeHasDialog(root);
  const headingLevelsInOrder: number[] = [];

  walk(root, (node) => {
    facts.nodeCount += 1;

    if (!resolveComponent(node.type)) {
      // Only meaningful when the registry is populated; otherwise every type
      // looks "unknown" and the guarding constraint stays inert.
      facts.unknownComponentCount += 1;
    }

    // Inputs
    if (INPUT_TYPES.has(node.type)) {
      facts.inputCount += 1;
      // Select uses placeholder/label too; all three expose `label`.
      if (!propProvided(node, 'label')) facts.inputsMissingLabel += 1;
    }

    // Buttons
    if (node.type === 'Button') {
      facts.buttonCount += 1;
      if (!propProvided(node, 'label')) facts.buttonsMissingLabel += 1;
      if (node.props?.variant === 'danger') {
        facts.dangerButtonCount += 1;
        if (!hasDialogSomewhere) facts.dangerButtonsWithoutConfirm += 1;
      }
    }

    // Links
    if (node.type === 'Link') {
      facts.linkCount += 1;
      if (node.props?.external === true) {
        facts.externalLinkCount += 1;
        if (!hasTextChild(node)) facts.externalLinksMissingText += 1;
      }
    }

    // Headings
    if (node.type === 'Heading') {
      facts.headingCount += 1;
      const level = Number(node.props?.level ?? 2);
      headingLevelsInOrder.push(level);
      if (level === 1) facts.h1Count += 1;
      if (level <= 2) facts.hasTopLevelHeading = true;
    }

    // Dialogs
    if (node.type === 'Dialog') {
      facts.dialogCount += 1;
      const hasConfirm = typeof node.props?.onConfirm !== 'undefined' || !!node.bindings?.onConfirm;
      const hasCancel = typeof node.props?.onCancel !== 'undefined' || !!node.bindings?.onCancel;
      if (!hasConfirm || !hasCancel) facts.dialogsMissingHandlers += 1;
    }

    // Images (Image component may be plugin-registered; check by type name)
    if (node.type === 'Image' || node.type === 'Img') {
      facts.imageCount += 1;
      if (!propProvided(node, 'alt')) facts.imagesMissingAlt += 1;
    }

    // Colour contrast (only when a theme mode is known — else the surface is
    // unknown and we honestly cannot check). Count CHECKABLE text nodes whose
    // colour fails WCAG AA against the active mode's surface. Unknown colours
    // (no token/hex) are skipped, never guessed.
    if (themeMode !== undefined && isTextKind(node)) {
      const fg = checkableTextColor(node, themeMode);
      if (fg !== null) {
        const bg = THEME_SURFACE[themeMode].background;
        if (!meetsContrast(fg, bg, WCAG_AA_NORMAL)) facts.lowContrastTextCount += 1;
      }
    }
  });

  // Derived booleans
  facts.allInputsLabeled = facts.inputsMissingLabel === 0;
  facts.allButtonsLabeled = facts.buttonsMissingLabel === 0;
  facts.allImagesHaveAlt = facts.imagesMissingAlt === 0;

  // Heading-level skip detection: any forward jump > 1 between consecutive
  // headings in document order (e.g. 2 → 4 skips 3). Going back up is fine.
  for (let i = 1; i < headingLevelsInOrder.length; i += 1) {
    const prev = headingLevelsInOrder[i - 1];
    const cur = headingLevelsInOrder[i];
    if (cur - prev > 1) {
      facts.headingsSkipLevel = true;
      break;
    }
  }

  return facts;
}
