import { describe, it, expect } from 'vitest';

// Inline the functions to test them in isolation
function resolveValue(raw: string, context: Record<string, unknown>): unknown {
  const trimmed = raw.trim();
  if (trimmed === 'true') return true;
  if (trimmed === 'false') return false;
  if (trimmed === 'null') return null;
  if (trimmed === 'undefined') return undefined;
  if (/^\d+(\.\d+)?$/.test(trimmed)) return Number(trimmed);
  if (/^["'].*["']$/.test(trimmed)) return trimmed.slice(1, -1);
  return resolvePath(trimmed, context);
}

function resolvePath(path: string, obj: Record<string, unknown>): unknown {
  const parts = path.split('.');
  let current: unknown = obj;
  for (const part of parts) {
    if (current == null || typeof current !== 'object') return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

function simpleEval(expr: string, context: Record<string, unknown>): boolean {
  const trimmed = expr.trim();
  if (trimmed === 'true') return true;
  if (trimmed === 'false') return false;

  if (trimmed.includes('===')) {
    const [lhs, rhs] = trimmed.split('===').map((s) => s.trim());
    const lhsVal = resolvePath(lhs, context);
    const rhsVal = resolveValue(rhs, context);
    return lhsVal === rhsVal;
  }

  if (trimmed.includes('!==')) {
    const [lhs, rhs] = trimmed.split('!==').map((s) => s.trim());
    const lhsVal = resolvePath(lhs, context);
    const rhsVal = resolveValue(rhs, context);
    return lhsVal !== rhsVal;
  }

  if (trimmed.includes('==')) {
    const [lhs, rhs] = trimmed.split('==').map((s) => s.trim());
    const lhsVal = resolvePath(lhs, context);
    const rhsClean = rhs.replace(/^["']|["']$/g, '');
    return String(lhsVal) === rhsClean;
  }

  if (trimmed.includes('!=')) {
    const [lhs, rhs] = trimmed.split('!=').map((s) => s.trim());
    const lhsVal = resolvePath(lhs, context);
    const rhsClean = rhs.replace(/^["']|["']$/g, '');
    return String(lhsVal) !== rhsClean;
  }

  const val = resolvePath(trimmed, context);
  return !!val;
}

describe('simpleEval', () => {
  it('evaluates === with boolean true', () => {
    expect(simpleEval("context.approved === true", { context: { approved: true } })).toBe(true);
  });

  it('evaluates === with boolean false (should not match true)', () => {
    expect(simpleEval("context.approved === true", { context: { approved: false } })).toBe(false);
  });

  it('evaluates === with string comparison', () => {
    expect(simpleEval("context.type === 'deployment'", { context: { type: 'deployment' } })).toBe(true);
  });

  it('evaluates !== correctly', () => {
    expect(simpleEval("context.status !== 'active'", { context: { status: 'inactive' } })).toBe(true);
  });

  it('evaluates == with string coercion', () => {
    expect(simpleEval("context.count == 5", { context: { count: 5 } })).toBe(true);
  });

  it('truthy check for bare path', () => {
    expect(simpleEval("context.enabled", { context: { enabled: true } })).toBe(true);
    expect(simpleEval("context.enabled", { context: { enabled: false } })).toBe(false);
  });

  it('handles the exact constraint scenario from baseline test', () => {
    // When: context.type === 'deployment' should be TRUE
    expect(simpleEval("context.type === 'deployment'", { context: { type: 'deployment', approved: false } })).toBe(true);
    // Require: context.approved === true should be FALSE
    expect(simpleEval("context.approved === true", { context: { type: 'deployment', approved: false } })).toBe(false);
  });
});
