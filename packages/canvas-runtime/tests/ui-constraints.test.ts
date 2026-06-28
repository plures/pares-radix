import { describe, it, expect, beforeAll } from 'vitest';
import { registerComponent } from '../src/registry.js';
import { validateUi, formatUiViolations, UI_CONSTRAINTS } from '../src/ui-constraints.js';
import type { CanvasNodeLike } from '../src/ui-facts.js';

// Register lightweight stand-ins (see ui-facts.test.ts for rationale).
// schemaKind matters for the contrast constraint: Text/Heading are 'display'
// (→ text-kind, contrast-checked), Box is 'layout' (→ container, not checked).
beforeAll(() => {
  const meta = {
    component: null as unknown as never,
    name: 'stub',
    category: 'display' as const,
    props: [],
    hasChildren: true,
    description: 'test stub',
  };
  for (const id of ['Input', 'TextArea', 'Select', 'Button', 'Link', 'Heading', 'Dialog', 'Image', 'Text']) {
    registerComponent(id, { ...meta, name: id });
  }
  registerComponent('Box', { ...meta, name: 'Box', category: 'layout' });
});

function tree(children: CanvasNodeLike[]): CanvasNodeLike {
  return { id: 'root', type: 'Box', children };
}

function names(violations: { constraint: string }[]): string[] {
  return violations.map((v) => v.constraint);
}

describe('validateUi — compliant tree', () => {
  it('a well-formed form passes with zero violations', () => {
    const result = validateUi(
      tree([
        { id: 'h', type: 'Heading', props: { level: 1 }, children: [] },
        { id: 'name', type: 'Input', props: { label: 'Name' } },
        { id: 'email', type: 'Input', props: { label: 'Email', type: 'email' } },
        { id: 'save', type: 'Button', props: { label: 'Save', variant: 'primary' } },
      ]),
    );
    expect(result.passed).toBe(true);
    expect(result.violations).toHaveLength(0);
  });
});

describe('validateUi — each violation class fires', () => {
  it('unlabeled input -> error', () => {
    const r = validateUi(tree([{ id: 'i', type: 'Input', props: {} }]));
    expect(names(r.violations)).toContain('ui_inputs_have_labels');
    expect(r.violations.find((v) => v.constraint === 'ui_inputs_have_labels')?.severity).toBe('error');
  });

  it('unlabeled button -> error', () => {
    const r = validateUi(tree([{ id: 'b', type: 'Button', props: {} }]));
    expect(names(r.violations)).toContain('ui_buttons_have_accessible_name');
  });

  it('image missing alt -> error', () => {
    const r = validateUi(tree([{ id: 'm', type: 'Image', props: { src: 'x.png' } }]));
    expect(names(r.violations)).toContain('ui_images_have_alt');
  });

  it('skipped heading level -> warning', () => {
    const r = validateUi(
      tree([
        { id: 'h2', type: 'Heading', props: { level: 2 }, children: [] },
        { id: 'h4', type: 'Heading', props: { level: 4 }, children: [] },
      ]),
    );
    expect(names(r.violations)).toContain('ui_no_skipped_heading_levels');
    expect(r.violations.find((v) => v.constraint === 'ui_no_skipped_heading_levels')?.severity).toBe(
      'warning',
    );
  });

  it('danger button without confirm Dialog -> warning', () => {
    const r = validateUi(
      tree([{ id: 'd', type: 'Button', props: { label: 'Delete', variant: 'danger' } }]),
    );
    expect(names(r.violations)).toContain('ui_destructive_actions_need_confirmation');
  });

  it('dialog missing handlers -> error', () => {
    const r = validateUi(
      tree([{ id: 'dlg', type: 'Dialog', props: { open: true, title: 't', message: 'm' } }]),
    );
    expect(names(r.violations)).toContain('ui_dialogs_are_actionable');
  });

  it('unknown component -> error', () => {
    const r = validateUi(tree([{ id: 'x', type: 'Frobnicator', props: {} }]));
    expect(names(r.violations)).toContain('ui_no_unknown_components');
  });

  it('multiple h1 -> warning', () => {
    const r = validateUi(
      tree([
        { id: 'a', type: 'Heading', props: { level: 1 }, children: [] },
        { id: 'b', type: 'Heading', props: { level: 1 }, children: [] },
      ]),
    );
    expect(names(r.violations)).toContain('ui_single_h1');
  });
});

describe('validateUi — ui_text_contrast_aa (WCAG AA, theme-aware)', () => {
  it('fires (error) on a low-contrast text node when a theme mode is supplied', () => {
    const r = validateUi(tree([{ id: 't', type: 'Text', props: { color: '#cccccc' } }]), {
      themeMode: 'light',
    });
    expect(names(r.violations)).toContain('ui_text_contrast_aa');
    expect(r.violations.find((v) => v.constraint === 'ui_text_contrast_aa')?.severity).toBe('error');
  });

  it('passes on a good-contrast token when a theme mode is supplied', () => {
    const r = validateUi(tree([{ id: 't', type: 'Text', themeToken: 'fg' }]), {
      themeMode: 'light',
    });
    expect(names(r.violations)).not.toContain('ui_text_contrast_aa');
  });

  it('is INERT (absent) when no theme mode is supplied, even with bad colour', () => {
    // contrastChecked === false → the when-guard is false → rule does not fire.
    // Honest "surface unknown" state, not a silent pass.
    const r = validateUi(tree([{ id: 't', type: 'Text', props: { color: '#cccccc' } }]));
    expect(r.facts.contrastChecked).toBe(false);
    expect(names(r.violations)).not.toContain('ui_text_contrast_aa');
  });
});

describe('formatUiViolations', () => {
  it('prefixes errors and warnings distinctly', () => {
    const r = validateUi(
      tree([
        { id: 'i', type: 'Input', props: {} }, // error
        { id: 'b', type: 'Button', props: { label: 'Delete', variant: 'danger' } }, // warning
      ]),
    );
    const lines = formatUiViolations(r.violations);
    expect(lines.some((l) => l.startsWith('[ui:error]'))).toBe(true);
    expect(lines.some((l) => l.startsWith('[ui:warn]'))).toBe(true);
  });
});

describe('validateUi — evaluated count matches constraint set', () => {
  it('evaluates every constraint', () => {
    const r = validateUi(tree([]));
    expect(r.evaluated).toBe(UI_CONSTRAINTS.length);
  });
});
