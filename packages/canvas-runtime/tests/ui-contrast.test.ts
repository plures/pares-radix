/**
 * ui-contrast.ts — WCAG colour-contrast math. Verified against the canonical
 * extreme pairs (#000/#fff = 21, identical = 1) and a mid pair computed from the
 * spec to a 0.1 tolerance.
 */
import { describe, it, expect } from 'vitest';
import {
  parseHexColor,
  relativeLuminance,
  contrastRatio,
  contrastRatioFromLuminance,
  meetsContrast,
  WCAG_AA_NORMAL,
  WCAG_AA_LARGE,
} from '../src/ui-contrast.js';

describe('parseHexColor', () => {
  it('parses #rrggbb', () => {
    expect(parseHexColor('#ff8800')).toEqual({ r: 255, g: 136, b: 0 });
  });
  it('parses without leading #', () => {
    expect(parseHexColor('00ff00')).toEqual({ r: 0, g: 255, b: 0 });
  });
  it('expands #rgb shorthand', () => {
    expect(parseHexColor('#f00')).toEqual({ r: 255, g: 0, b: 0 });
  });
  it('parses #rrggbbaa (ignores alpha for the triple)', () => {
    expect(parseHexColor('#11223344')).toEqual({ r: 0x11, g: 0x22, b: 0x33 });
  });
  it('returns null for garbage', () => {
    expect(parseHexColor('not-a-color')).toBeNull();
    expect(parseHexColor('#12')).toBeNull();
    expect(parseHexColor('#xyzxyz')).toBeNull();
  });
});

describe('relativeLuminance', () => {
  it('black = 0, white = 1', () => {
    expect(relativeLuminance({ r: 0, g: 0, b: 0 })).toBeCloseTo(0, 6);
    expect(relativeLuminance({ r: 255, g: 255, b: 255 })).toBeCloseTo(1, 6);
  });
  it('green carries the most luminance weight of the primaries', () => {
    const r = relativeLuminance({ r: 255, g: 0, b: 0 });
    const g = relativeLuminance({ r: 0, g: 255, b: 0 });
    const b = relativeLuminance({ r: 0, g: 0, b: 255 });
    expect(g).toBeGreaterThan(r);
    expect(r).toBeGreaterThan(b);
  });
});

describe('contrastRatio — known pairs', () => {
  it('#000 vs #fff = 21:1 (the maximum)', () => {
    expect(contrastRatio('#000000', '#ffffff')).toBeCloseTo(21, 5);
  });
  it('identical colours = 1:1 (the minimum)', () => {
    expect(contrastRatio('#777777', '#777777')).toBeCloseTo(1, 6);
    expect(contrastRatio('#1d4ed8', '#1d4ed8')).toBeCloseTo(1, 6);
  });
  it('is symmetric (order of fg/bg does not matter)', () => {
    const a = contrastRatio('#112233', '#ddeeff')!;
    const b = contrastRatio('#ddeeff', '#112233')!;
    expect(a).toBeCloseTo(b, 10);
  });
  it('a mid pair matches the spec value within 0.1', () => {
    // #777777 vs #ffffff: known WCAG ratio ≈ 4.48 (per WebAIM contrast checker).
    // Asserted to an explicit 0.1 absolute tolerance (the brief's mid-pair check).
    const mid = contrastRatio('#777777', '#ffffff')!;
    expect(Math.abs(mid - 4.48)).toBeLessThanOrEqual(0.1);
    // #1d4ed8 (accent light) vs #ffffff ≈ 6.70 (computed from the WCAG formula).
    const accent = contrastRatio('#1d4ed8', '#ffffff')!;
    expect(Math.abs(accent - 6.7)).toBeLessThanOrEqual(0.1);
  });
  it('returns null when a colour cannot be parsed', () => {
    expect(contrastRatio('#000000', 'bogus')).toBeNull();
    expect(contrastRatio('bogus', '#ffffff')).toBeNull();
  });
});

describe('contrastRatioFromLuminance', () => {
  it('matches the (L1+0.05)/(L2+0.05) formula at the extremes', () => {
    expect(contrastRatioFromLuminance(1, 0)).toBeCloseTo(21, 6);
    expect(contrastRatioFromLuminance(0.5, 0.5)).toBeCloseTo(1, 6);
  });
});

describe('meetsContrast', () => {
  it('#000/#fff clears AA normal and AA large', () => {
    expect(meetsContrast('#000000', '#ffffff', WCAG_AA_NORMAL)).toBe(true);
    expect(meetsContrast('#000000', '#ffffff', WCAG_AA_LARGE)).toBe(true);
  });
  it('a low-contrast pair fails AA normal', () => {
    // #aaaaaa on #ffffff ≈ 1.99 — below 4.5.
    expect(meetsContrast('#aaaaaa', '#ffffff', WCAG_AA_NORMAL)).toBe(false);
  });
  it('defaults to AA normal threshold', () => {
    expect(meetsContrast('#000000', '#ffffff')).toBe(true);
  });
  it('fails closed on unparseable input (unknown colour does not "pass")', () => {
    expect(meetsContrast('bogus', '#ffffff')).toBe(false);
  });
});
