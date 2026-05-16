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

  // Handle && (logical AND)
  if (trimmed.includes(' && ')) {
    const parts = trimmed.split(' && ');
    return parts.every((part) => simpleEval(part, context));
  }

  // Handle || (logical OR)
  if (trimmed.includes(' || ')) {
    const parts = trimmed.split(' || ');
    return parts.some((part) => simpleEval(part, context));
  }

  // Handle negation prefix: !expr
  if (trimmed.startsWith('!') && !trimmed.startsWith('!=')) {
    return !simpleEval(trimmed.slice(1), context);
  }

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

  if (trimmed.includes('>=')) {
    const [lhs, rhs] = trimmed.split('>=').map((s) => s.trim());
    return Number(resolvePath(lhs, context)) >= Number(resolveValue(rhs, context));
  }

  if (trimmed.includes('<=')) {
    const [lhs, rhs] = trimmed.split('<=').map((s) => s.trim());
    return Number(resolvePath(lhs, context)) <= Number(resolveValue(rhs, context));
  }

  if (trimmed.includes('>')) {
    const [lhs, rhs] = trimmed.split('>').map((s) => s.trim());
    return Number(resolvePath(lhs, context)) > Number(resolveValue(rhs, context));
  }

  if (trimmed.includes('<')) {
    const [lhs, rhs] = trimmed.split('<').map((s) => s.trim());
    return Number(resolvePath(lhs, context)) < Number(resolveValue(rhs, context));
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

  // ── Compound expressions (&&, ||) ──

  it('evaluates && with both conditions true', () => {
    expect(simpleEval("context.type === 'deployment' && context.approved === true",
      { context: { type: 'deployment', approved: true } })).toBe(true);
  });

  it('evaluates && with one condition false', () => {
    expect(simpleEval("context.type === 'deployment' && context.approved === true",
      { context: { type: 'deployment', approved: false } })).toBe(false);
  });

  it('evaluates || with one condition true', () => {
    expect(simpleEval("context.env === 'prod' || context.env === 'staging'",
      { context: { env: 'staging' } })).toBe(true);
  });

  it('evaluates || with both conditions false', () => {
    expect(simpleEval("context.env === 'prod' || context.env === 'staging'",
      { context: { env: 'dev' } })).toBe(false);
  });

  it('evaluates && with three conditions', () => {
    expect(simpleEval("context.a === 1 && context.b === 2 && context.c === 3",
      { context: { a: 1, b: 2, c: 3 } })).toBe(true);
    expect(simpleEval("context.a === 1 && context.b === 2 && context.c === 3",
      { context: { a: 1, b: 2, c: 99 } })).toBe(false);
  });

  // ── Negation ──

  it('evaluates ! negation on truthy path', () => {
    expect(simpleEval("!context.disabled", { context: { disabled: false } })).toBe(true);
    expect(simpleEval("!context.disabled", { context: { disabled: true } })).toBe(false);
  });

  // ── Comparison operators ──

  it('evaluates > correctly', () => {
    expect(simpleEval("context.count > 5", { context: { count: 10 } })).toBe(true);
    expect(simpleEval("context.count > 5", { context: { count: 3 } })).toBe(false);
  });

  it('evaluates >= correctly', () => {
    expect(simpleEval("context.count >= 5", { context: { count: 5 } })).toBe(true);
    expect(simpleEval("context.count >= 5", { context: { count: 4 } })).toBe(false);
  });

  it('evaluates < correctly', () => {
    expect(simpleEval("context.count < 5", { context: { count: 3 } })).toBe(true);
    expect(simpleEval("context.count < 5", { context: { count: 5 } })).toBe(false);
  });

  it('evaluates <= correctly', () => {
    expect(simpleEval("context.count <= 5", { context: { count: 5 } })).toBe(true);
    expect(simpleEval("context.count <= 5", { context: { count: 6 } })).toBe(false);
  });

  // ── Combined: compound + comparison ──

  it('evaluates compound with comparison operators', () => {
    expect(simpleEval("context.type === 'deployment' && context.replicas >= 3",
      { context: { type: 'deployment', replicas: 5 } })).toBe(true);
    expect(simpleEval("context.type === 'deployment' && context.replicas >= 3",
      { context: { type: 'deployment', replicas: 1 } })).toBe(false);
  });
});
