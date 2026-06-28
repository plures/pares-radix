/**
 * ui-contrast.ts — pure WCAG colour-contrast math.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * WHY THIS EXISTS
 * The theme practices (ui-theme.px) map semantic tokens → concrete colours per
 * mode. To make those palettes HONEST (and to power a future validate-half
 * contrast guard), we need real, testable contrast math — not a stub. This module
 * is the pure WCAG 2.x implementation: sRGB → relative luminance → contrast ratio
 * between two colours.
 *
 * Spec: https://www.w3.org/TR/WCAG21/#dfn-relative-luminance and #dfn-contrast-ratio
 *
 * PURE & FLAT: no IO, no DOM, no allocation beyond locals. Hex in, number out.
 * ─────────────────────────────────────────────────────────────────────────────
 */

/** An RGB triple with 0–255 integer channels. */
export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/**
 * Parse a hex colour string into 0–255 channels. Accepts `#rgb`, `#rrggbb`,
 * `#rgba`, `#rrggbbaa` (and the same without the leading `#`); alpha is parsed
 * but ignored for luminance (contrast is defined on opaque colours). Returns
 * `null` for anything it cannot parse, so callers can fail honestly rather than
 * compute a bogus ratio.
 */
export function parseHexColor(input: string): Rgb | null {
  if (typeof input !== 'string') return null;
  let hex = input.trim();
  if (hex.startsWith('#')) hex = hex.slice(1);
  // Expand shorthand (#rgb / #rgba → #rrggbb / #rrggbbaa).
  if (hex.length === 3 || hex.length === 4) {
    hex = hex
      .split('')
      .map((ch) => ch + ch)
      .join('');
  }
  if (hex.length !== 6 && hex.length !== 8) return null;
  if (!/^[0-9a-fA-F]+$/.test(hex)) return null;
  const r = Number.parseInt(hex.slice(0, 2), 16);
  const g = Number.parseInt(hex.slice(2, 4), 16);
  const b = Number.parseInt(hex.slice(4, 6), 16);
  if (Number.isNaN(r) || Number.isNaN(g) || Number.isNaN(b)) return null;
  return { r, g, b };
}

/**
 * Linearize one sRGB channel (0–255) to its 0–1 linear-light value, per the
 * WCAG/sRGB transfer function.
 */
function linearizeChannel(channel8: number): number {
  const c = channel8 / 255;
  return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}

/**
 * WCAG relative luminance (0 = black, 1 = white) of an sRGB colour.
 *
 * L = 0.2126·R + 0.7152·G + 0.0722·B  (R,G,B linearized)
 */
export function relativeLuminance(color: Rgb): number {
  const r = linearizeChannel(color.r);
  const g = linearizeChannel(color.g);
  const b = linearizeChannel(color.b);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/**
 * WCAG contrast ratio between two relative luminances.
 * (Lighter + 0.05) / (Darker + 0.05) — range 1 (identical) … 21 (#000 vs #fff).
 */
export function contrastRatioFromLuminance(l1: number, l2: number): number {
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

/**
 * WCAG contrast ratio between two hex colours (e.g. text vs background).
 *
 * @returns the ratio in [1, 21], or `null` if either colour cannot be parsed
 *          (honest failure — callers must not treat null as "passes").
 * @example contrastRatio('#000000', '#ffffff') === 21
 * @example contrastRatio('#777', '#777') === 1
 */
export function contrastRatio(fg: string, bg: string): number | null {
  const a = parseHexColor(fg);
  const b = parseHexColor(bg);
  if (!a || !b) return null;
  return contrastRatioFromLuminance(relativeLuminance(a), relativeLuminance(b));
}

/** WCAG 2.x AA threshold for normal-size body text. */
export const WCAG_AA_NORMAL = 4.5;
/** WCAG 2.x AA threshold for large text (≥18pt / ≥14pt bold). */
export const WCAG_AA_LARGE = 3;
/** WCAG 2.x AAA threshold for normal-size body text. */
export const WCAG_AAA_NORMAL = 7;

/**
 * Does the fg/bg pair meet a WCAG threshold? Returns `false` on unparseable
 * input (fail-closed: an unknown colour does not "pass" contrast).
 */
export function meetsContrast(fg: string, bg: string, threshold: number = WCAG_AA_NORMAL): boolean {
  const ratio = contrastRatio(fg, bg);
  if (ratio === null) return false;
  return ratio >= threshold;
}
