import { describe, it, expect, beforeAll } from 'vitest';
import { registerComponent } from '../src/registry.js';
import { extractUiFacts } from '../src/ui-facts.js';
import type { CanvasNodeLike } from '../src/ui-facts.js';

// Register lightweight stand-ins so resolveComponent() succeeds for the types
// these tests use. We avoid registerDesignDojo() because it dynamically imports
// .svelte files, which the plain node test loader cannot parse. The extractor
// only needs the registry to know whether a type is registered, not the actual
// Svelte component — so a null component with the right id is sufficient.
beforeAll(() => {
  const meta = {
    component: null as unknown as never,
    name: 'stub',
    category: 'display' as const,
    props: [],
    hasChildren: true,
    description: 'test stub',
  };
  for (const id of ['Box', 'Input', 'TextArea', 'Select', 'Button', 'Link', 'Heading', 'Dialog', 'Image']) {
    registerComponent(id, { ...meta, name: id });
  }
});

function box(children: CanvasNodeLike[]): CanvasNodeLike {
  return { id: 'root', type: 'Box', children };
}

describe('extractUiFacts — empty / null', () => {
  it('null tree yields all-compliant zero facts', () => {
    const f = extractUiFacts(null);
    expect(f.nodeCount).toBe(0);
    expect(f.allInputsLabeled).toBe(true);
    expect(f.allButtonsLabeled).toBe(true);
    expect(f.allImagesHaveAlt).toBe(true);
    expect(f.headingsSkipLevel).toBe(false);
  });
});

describe('extractUiFacts — inputs', () => {
  it('counts labeled vs unlabeled inputs', () => {
    const f = extractUiFacts(
      box([
        { id: 'a', type: 'Input', props: { label: 'Name' } },
        { id: 'b', type: 'Input', props: {} },
        { id: 'c', type: 'TextArea', props: { label: 'Bio' } },
        { id: 'd', type: 'Select', props: { options: [] } }, // no label
      ]),
    );
    expect(f.inputCount).toBe(4);
    expect(f.inputsMissingLabel).toBe(2);
    expect(f.allInputsLabeled).toBe(false);
  });

  it('treats a bound label as provided', () => {
    const f = extractUiFacts(
      box([{ id: 'a', type: 'Input', props: {}, bindings: { label: 'user.nameLabel' } }]),
    );
    expect(f.inputsMissingLabel).toBe(0);
    expect(f.allInputsLabeled).toBe(true);
  });
});

describe('extractUiFacts — buttons & destructive actions', () => {
  it('flags unlabeled buttons', () => {
    const f = extractUiFacts(
      box([
        { id: 'a', type: 'Button', props: { label: 'Save' } },
        { id: 'b', type: 'Button', props: {} },
      ]),
    );
    expect(f.buttonCount).toBe(2);
    expect(f.buttonsMissingLabel).toBe(1);
    expect(f.allButtonsLabeled).toBe(false);
  });

  it('danger button without a Dialog is flagged', () => {
    const f = extractUiFacts(
      box([{ id: 'a', type: 'Button', props: { label: 'Delete', variant: 'danger' } }]),
    );
    expect(f.dangerButtonCount).toBe(1);
    expect(f.dangerButtonsWithoutConfirm).toBe(1);
  });

  it('danger button WITH a Dialog in the tree is satisfied', () => {
    const f = extractUiFacts(
      box([
        { id: 'a', type: 'Button', props: { label: 'Delete', variant: 'danger' } },
        {
          id: 'd',
          type: 'Dialog',
          props: { open: false, title: 'Sure?', message: 'x', onConfirm: 'p', onCancel: 'p' },
        },
      ]),
    );
    expect(f.dangerButtonsWithoutConfirm).toBe(0);
  });
});

describe('extractUiFacts — headings hierarchy', () => {
  it('detects skipped heading levels (2 -> 4)', () => {
    const f = extractUiFacts(
      box([
        { id: 'h2', type: 'Heading', props: { level: 2 } },
        { id: 'h4', type: 'Heading', props: { level: 4 } },
      ]),
    );
    expect(f.headingsSkipLevel).toBe(true);
    expect(f.hasTopLevelHeading).toBe(true);
  });

  it('consecutive step-by-one levels do not count as skip', () => {
    const f = extractUiFacts(
      box([
        { id: 'h1', type: 'Heading', props: { level: 1 } },
        { id: 'h2', type: 'Heading', props: { level: 2 } },
        { id: 'h3', type: 'Heading', props: { level: 3 } },
        { id: 'h2b', type: 'Heading', props: { level: 2 } }, // going back up is fine
      ]),
    );
    expect(f.headingsSkipLevel).toBe(false);
    expect(f.h1Count).toBe(1);
  });

  it('no top-level heading when starting at h3', () => {
    const f = extractUiFacts(box([{ id: 'h3', type: 'Heading', props: { level: 3 } }]));
    expect(f.hasTopLevelHeading).toBe(false);
  });
});

describe('extractUiFacts — dialogs, images, unknown', () => {
  it('flags dialogs missing handlers', () => {
    const f = extractUiFacts(
      box([{ id: 'd', type: 'Dialog', props: { open: true, title: 't', message: 'm' } }]),
    );
    expect(f.dialogCount).toBe(1);
    expect(f.dialogsMissingHandlers).toBe(1);
  });

  it('flags images without alt', () => {
    const f = extractUiFacts(
      box([
        { id: 'i1', type: 'Image', props: { src: 'a.png', alt: 'A cat' } },
        { id: 'i2', type: 'Image', props: { src: 'b.png' } },
      ]),
    );
    expect(f.imageCount).toBe(2);
    expect(f.imagesMissingAlt).toBe(1);
    expect(f.allImagesHaveAlt).toBe(false);
  });

  it('counts unknown component types', () => {
    const f = extractUiFacts(
      box([
        { id: 'ok', type: 'Button', props: { label: 'Hi' } },
        { id: 'bad', type: 'NotARealComponent', props: {} },
      ]),
    );
    expect(f.unknownComponentCount).toBe(1);
  });
});
