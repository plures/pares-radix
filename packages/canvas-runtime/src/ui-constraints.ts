/**
 * ui-constraints.ts — Runtime UI best-practice constraints + validator.
 *
 * These mirror `praxis/ui/ui-best-practices.px` exactly (same names, `when`,
 * `require`, `severity`, `message`). The `.px` file is the human-readable
 * source of truth and the artifact loaded into PluresDB for `praxis.evaluate`;
 * this TS copy lets `canvas-runtime` validate a canvas WITHOUT a running Praxis
 * engine (e.g. in `canvas.validate`, in CI, in unit tests).
 *
 * DRIFT GUARD: `ui-constraints.sync.test.ts` parses the `.px` file and asserts
 * this array stays identical (constraint names + require expressions). If you
 * edit one, edit both — the test fails otherwise. This honors C-DRIFT-001
 * (artifact derived from source must be enforced, never manually kept in sync
 * by memory).
 *
 * Evaluation uses the same flat-boolean semantics as the MCP server's
 * `simpleEval`, reimplemented here as `evalExpr` so the two agree.
 */

import { extractUiFacts, type CanvasNodeLike, type UiFacts } from './ui-facts.js';

export type Severity = 'error' | 'warning';

export interface UiConstraint {
  name: string;
  when: string;
  require: string;
  severity: Severity;
  message: string;
}

export interface UiViolation {
  constraint: string;
  severity: Severity;
  message: string;
}

export interface UiValidationResult {
  facts: UiFacts;
  evaluated: number;
  violations: UiViolation[];
  passed: boolean;
}

/**
 * The constraint set. Mirrors praxis/ui/ui-best-practices.px. All `when`/
 * `require` expressions reference `context.ui.<fact>` produced by extractUiFacts.
 */
export const UI_CONSTRAINTS: readonly UiConstraint[] = [
  {
    name: 'ui_inputs_have_labels',
    when: 'context.ui.inputCount > 0',
    require: 'context.ui.allInputsLabeled === true',
    severity: 'error',
    message:
      'Every form input (Input/TextArea/Select) must have a label. Unlabeled inputs are invisible to screen readers and ambiguous to everyone. Add a `label` prop (or bind one).',
  },
  {
    name: 'ui_buttons_have_accessible_name',
    when: 'context.ui.buttonCount > 0',
    require: 'context.ui.allButtonsLabeled === true',
    severity: 'error',
    message:
      'Every Button must have a non-empty `label` (its accessible name). A button with no name cannot be announced, found by voice control, or understood.',
  },
  {
    name: 'ui_images_have_alt',
    when: 'context.ui.imageCount > 0',
    require: 'context.ui.allImagesHaveAlt === true',
    severity: 'error',
    message:
      "Every image must have an `alt` text alternative (empty alt='' only for purely decorative images). Missing alt text fails WCAG 1.1.1.",
  },
  {
    name: 'ui_external_links_have_text',
    when: 'context.ui.externalLinkCount > 0',
    require: 'context.ui.externalLinksMissingText === 0',
    severity: 'error',
    message:
      'External links must contain visible link text. An empty link has no accessible name and no click affordance.',
  },
  {
    name: 'ui_has_top_level_heading',
    when: 'context.ui.headingCount > 0',
    require: 'context.ui.hasTopLevelHeading === true',
    severity: 'warning',
    message:
      "A view with headings should have a top-level heading (level 1 or 2) so screen-reader users can orient. Don't start the hierarchy at h3+.",
  },
  {
    name: 'ui_no_skipped_heading_levels',
    when: 'context.ui.headingCount > 1',
    require: 'context.ui.headingsSkipLevel === false',
    severity: 'warning',
    message:
      'Heading levels must not skip (e.g. h2 → h4). Skipped levels break the document outline that assistive tech relies on. Demote/promote so levels step by one.',
  },
  {
    name: 'ui_single_h1',
    when: 'context.ui.headingCount > 0',
    require: 'context.ui.h1Count <= 1',
    severity: 'warning',
    message:
      "Prefer a single level-1 heading per view (the page's one title). Multiple h1s dilute the document outline.",
  },
  {
    name: 'ui_destructive_actions_need_confirmation',
    when: 'context.ui.dangerButtonCount > 0',
    require: 'context.ui.dangerButtonsWithoutConfirm === 0',
    severity: 'warning',
    message:
      "Destructive (variant='danger') actions should be paired with a confirmation Dialog. Irreversible actions one click away cause data loss. Add a Dialog to the view.",
  },
  {
    name: 'ui_dialogs_are_actionable',
    when: 'context.ui.dialogCount > 0',
    require: 'context.ui.dialogsMissingHandlers === 0',
    severity: 'error',
    message:
      'Every Dialog must wire both onConfirm and onCancel. A modal with no actions traps the user (no way to proceed or escape) — a dead-end and a focus trap.',
  },
  {
    name: 'ui_no_unknown_components',
    when: 'context.ui.registrySize > 0',
    require: 'context.ui.unknownComponentCount === 0',
    severity: 'error',
    message:
      'Every node `type` must resolve to a registered component. An unknown type renders nothing (silent blank). Register the component or fix the type name.',
  },
] as const;

// ── Flat-boolean evaluator (semantics match MCP server simpleEval) ───────────

function resolvePath(path: string, obj: Record<string, unknown>): unknown {
  const parts = path.split('.');
  let current: unknown = obj;
  for (const part of parts) {
    if (current == null || typeof current !== 'object') return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

function resolveValue(raw: string, context: Record<string, unknown>): unknown {
  const trimmed = raw.trim();
  if (trimmed === 'true') return true;
  if (trimmed === 'false') return false;
  if (trimmed === 'null') return null;
  if (trimmed === 'undefined') return undefined;
  if (/^-?\d+(\.\d+)?$/.test(trimmed)) return Number(trimmed);
  if (/^["'].*["']$/.test(trimmed)) return trimmed.slice(1, -1);
  return resolvePath(trimmed, context);
}

export function evalExpr(expr: string, context: Record<string, unknown>): boolean {
  const trimmed = expr.trim();
  if (trimmed === 'true') return true;
  if (trimmed === 'false') return false;

  if (trimmed.includes(' && ')) {
    return trimmed.split(' && ').every((p) => evalExpr(p, context));
  }
  if (trimmed.includes(' || ')) {
    return trimmed.split(' || ').some((p) => evalExpr(p, context));
  }
  if (trimmed.startsWith('!') && !trimmed.startsWith('!=')) {
    return !evalExpr(trimmed.slice(1), context);
  }
  if (trimmed.includes('===')) {
    const [l, r] = trimmed.split('===').map((s) => s.trim());
    return resolvePath(l, context) === resolveValue(r, context);
  }
  if (trimmed.includes('!==')) {
    const [l, r] = trimmed.split('!==').map((s) => s.trim());
    return resolvePath(l, context) !== resolveValue(r, context);
  }
  if (trimmed.includes('>=')) {
    const [l, r] = trimmed.split('>=').map((s) => s.trim());
    return Number(resolvePath(l, context)) >= Number(resolveValue(r, context));
  }
  if (trimmed.includes('<=')) {
    const [l, r] = trimmed.split('<=').map((s) => s.trim());
    return Number(resolvePath(l, context)) <= Number(resolveValue(r, context));
  }
  if (trimmed.includes('>')) {
    const [l, r] = trimmed.split('>').map((s) => s.trim());
    return Number(resolvePath(l, context)) > Number(resolveValue(r, context));
  }
  if (trimmed.includes('<')) {
    const [l, r] = trimmed.split('<').map((s) => s.trim());
    return Number(resolvePath(l, context)) < Number(resolveValue(r, context));
  }
  if (trimmed.includes('==')) {
    const [l, r] = trimmed.split('==').map((s) => s.trim());
    return String(resolvePath(l, context)) === r.replace(/^["']|["']$/g, '');
  }
  if (trimmed.includes('!=')) {
    const [l, r] = trimmed.split('!=').map((s) => s.trim());
    return String(resolvePath(l, context)) !== r.replace(/^["']|["']$/g, '');
  }
  return !!resolvePath(trimmed, context);
}

// ── Public validator ─────────────────────────────────────────────────────────

/**
 * Validate a canvas tree against the UI best-practice constraints.
 *
 * @param root The canvas root node (or `doc.tree`).
 * @returns facts + violations. `passed` is true when there are no violations.
 */
export function validateUi(root: CanvasNodeLike | null | undefined): UiValidationResult {
  const facts = extractUiFacts(root);
  const scope = { context: { ui: facts } } as Record<string, unknown>;
  const violations: UiViolation[] = [];

  for (const c of UI_CONSTRAINTS) {
    if (c.when && !evalExpr(c.when, scope)) continue;
    if (c.require && !evalExpr(c.require, scope)) {
      violations.push({ constraint: c.name, severity: c.severity, message: c.message });
    }
  }

  return {
    facts,
    evaluated: UI_CONSTRAINTS.length,
    violations,
    passed: violations.length === 0,
  };
}

/**
 * Format UI violations as human-readable issue strings (for the `string[]`
 * `validateCanvas` contract). Errors are prefixed `[ui:error]`, warnings
 * `[ui:warn]`.
 */
export function formatUiViolations(violations: UiViolation[]): string[] {
  return violations.map(
    (v) => `[ui:${v.severity === 'error' ? 'error' : 'warn'}] ${v.constraint}: ${v.message}`,
  );
}
