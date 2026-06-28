import { describe, it, expect } from 'vitest';
import {
  SCHEMA_KINDS,
  kindForComponent,
  RESPONSIVE_ATTRS,
  RESPONSIVE_ATTR_SET,
  BREAKPOINTS,
  BREAKPOINT_ORDER,
  breakpointFor,
  pickResponsive,
  type SchemaKind,
} from '../src/ui-schema.js';
import type { ComponentMeta } from '../src/registry.js';

describe('ui-schema — breakpointFor', () => {
  it('maps boundary widths to the correct breakpoint', () => {
    expect(breakpointFor(0)).toBe('base');
    expect(breakpointFor(639)).toBe('base');
    expect(breakpointFor(640)).toBe('sm');
    expect(breakpointFor(767)).toBe('sm');
    expect(breakpointFor(768)).toBe('md');
    expect(breakpointFor(1023)).toBe('md');
    expect(breakpointFor(1024)).toBe('lg');
    expect(breakpointFor(1279)).toBe('lg');
    expect(breakpointFor(1280)).toBe('xl');
    expect(breakpointFor(4000)).toBe('xl');
  });

  it('handles negative/zero widths as base', () => {
    expect(breakpointFor(-100)).toBe('base');
  });

  it('BREAKPOINT_ORDER is smallest→largest and matches BREAKPOINTS', () => {
    expect(BREAKPOINT_ORDER).toEqual(['base', 'sm', 'md', 'lg', 'xl']);
    expect(BREAKPOINTS.map((b) => b.name)).toEqual([...BREAKPOINT_ORDER]);
    // strictly increasing min widths
    for (let i = 1; i < BREAKPOINTS.length; i++) {
      expect(BREAKPOINTS[i].min).toBeGreaterThan(BREAKPOINTS[i - 1].min);
    }
  });
});

describe('ui-schema — pickResponsive', () => {
  it('returns the exact value when the active breakpoint is defined', () => {
    expect(pickResponsive({ base: 'column', md: 'row' }, 'md')).toBe('row');
    expect(pickResponsive({ base: 'column', md: 'row' }, 'lg')).toBe('row'); // cascade up
    expect(pickResponsive({ base: 'column', md: 'row' }, 'xl')).toBe('row');
  });

  it('falls back to the nearest smaller defined breakpoint (mobile-first)', () => {
    expect(pickResponsive({ base: 'column', lg: 'row' }, 'md')).toBe('column');
    expect(pickResponsive({ base: '8px', md: '16px' }, 'sm')).toBe('8px');
  });

  it('returns undefined when nothing is defined at or below the active bp', () => {
    expect(pickResponsive({ md: 'row' }, 'sm')).toBeUndefined();
    expect(pickResponsive({ lg: 'row' }, 'base')).toBeUndefined();
  });

  it('returns undefined for an undefined/empty map', () => {
    expect(pickResponsive(undefined, 'md')).toBeUndefined();
    expect(pickResponsive({}, 'md')).toBeUndefined();
  });

  it('preserves falsy values (false/0/empty-string) rather than skipping them', () => {
    expect(pickResponsive({ base: false } as Record<string, boolean>, 'lg')).toBe(false);
    expect(pickResponsive({ base: 0 } as Record<string, number>, 'lg')).toBe(0);
    expect(pickResponsive({ base: '' } as Record<string, string>, 'lg')).toBe('');
  });
});

describe('ui-schema — RESPONSIVE_ATTRS', () => {
  it('set mirrors the array exactly', () => {
    expect(RESPONSIVE_ATTR_SET.size).toBe(RESPONSIVE_ATTRS.length);
    for (const a of RESPONSIVE_ATTRS) expect(RESPONSIVE_ATTR_SET.has(a)).toBe(true);
  });

  it('includes the core layout attributes', () => {
    for (const a of ['direction', 'padding', 'gap', 'hidden', 'columns']) {
      expect(RESPONSIVE_ATTR_SET.has(a)).toBe(true);
    }
  });
});

describe('ui-schema — kindForComponent', () => {
  const cat = (c: ComponentMeta['category']) => kindForComponent(undefined, c);

  it('infers kind from category by default', () => {
    expect(cat('layout')).toBe('container');
    expect(cat('display')).toBe('text');
    expect(cat('input')).toBe('control');
    expect(cat('navigation')).toBe('navigation');
    expect(cat('feedback')).toBe('feedback');
    expect(cat('data')).toBe('group');
    expect(cat('custom')).toBe('container');
  });

  it('honors an explicit override over the category default', () => {
    // a 'display' component that should be treated as a container for layout rules
    expect(kindForComponent('container', 'display')).toBe('container');
    expect(kindForComponent('media', 'display')).toBe('media');
  });

  it('every produced kind is a member of SCHEMA_KINDS', () => {
    const cats: ComponentMeta['category'][] = [
      'layout', 'display', 'input', 'navigation', 'feedback', 'data', 'custom',
    ];
    for (const c of cats) {
      expect(SCHEMA_KINDS).toContain(cat(c));
    }
  });

  it('SCHEMA_KINDS is the full closed set', () => {
    const expected: SchemaKind[] = [
      'container', 'text', 'control', 'media', 'navigation', 'group', 'feedback',
    ];
    expect([...SCHEMA_KINDS].sort()).toEqual([...expected].sort());
  });
});
