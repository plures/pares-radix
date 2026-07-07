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

function resolveNumeric(expr: string, context: Record<string, unknown>): number {
  let s = expr.trim();
  while (s.startsWith('(') && s.endsWith(')')) {
    s = s.slice(1, -1).trim();
  }
  if (!/[+\-*/]/.test(s.replace(/^-/, ''))) {
    return Number(resolveValue(s, context));
  }
  const tokens = s.match(/[+\-*/]|[^+\-*/\s]+/g);
  if (!tokens || tokens.length === 0) return Number.NaN;
  const resolved = tokens
    .map((t) => (/^[+\-*/]$/.test(t) ? t : String(Number(resolveValue(t, context)))))
    .join(' ');
  if (!/^[-+*/.\d\s]+$/.test(resolved)) return Number.NaN;
  try {
    // eslint-disable-next-line no-new-func
    const val = Function(`"use strict"; return (${resolved});`)() as unknown;
    return typeof val === 'number' ? val : Number.NaN;
  } catch {
    return Number.NaN;
  }
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

  // Handle Array.includes(x)
  const includesMatch = trimmed.match(/^(.+)\.includes\((.*)\)$/);
  if (includesMatch) {
    const arrVal = resolvePath(includesMatch[1].trim(), context);
    const needle = resolveValue(includesMatch[2].trim(), context);
    return Array.isArray(arrVal) ? arrVal.includes(needle) : false;
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
    return resolveNumeric(lhs, context) >= resolveNumeric(rhs, context);
  }

  if (trimmed.includes('<=')) {
    const [lhs, rhs] = trimmed.split('<=').map((s) => s.trim());
    return resolveNumeric(lhs, context) <= resolveNumeric(rhs, context);
  }

  if (trimmed.includes('>')) {
    const [lhs, rhs] = trimmed.split('>').map((s) => s.trim());
    return resolveNumeric(lhs, context) > resolveNumeric(rhs, context);
  }

  if (trimmed.includes('<')) {
    const [lhs, rhs] = trimmed.split('<').map((s) => s.trim());
    return resolveNumeric(lhs, context) < resolveNumeric(rhs, context);
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

  // ── Array.includes(x) ──

  it('evaluates Array.includes(path) membership', () => {
    expect(simpleEval('policy.tickerAllowlist.includes(trade.symbol)',
      { policy: { tickerAllowlist: ['AAPL', 'MSFT'] }, trade: { symbol: 'AAPL' } })).toBe(true);
    expect(simpleEval('policy.tickerAllowlist.includes(trade.symbol)',
      { policy: { tickerAllowlist: ['AAPL', 'MSFT'] }, trade: { symbol: 'TSLA' } })).toBe(false);
  });

  it('evaluates Array.includes(literal)', () => {
    expect(simpleEval("policy.allowedSides.includes('buy')",
      { policy: { allowedSides: ['buy', 'sell'] } })).toBe(true);
    expect(simpleEval("policy.allowedSides.includes('short')",
      { policy: { allowedSides: ['buy', 'sell'] } })).toBe(false);
  });

  it('includes() on a non-array is false, never throws', () => {
    expect(simpleEval('policy.missing.includes(trade.symbol)',
      { policy: {}, trade: { symbol: 'AAPL' } })).toBe(false);
  });

  // ── Arithmetic operands in numeric comparisons ──

  it('evaluates parenthesized arithmetic operand: (a + b) <= c', () => {
    expect(simpleEval('(trade.dailySpentUsd + trade.notionalUsd) <= policy.dailyMaxUsd',
      { trade: { dailySpentUsd: 90000, notionalUsd: 5000 }, policy: { dailyMaxUsd: 100000 } })).toBe(true);
    expect(simpleEval('(trade.dailySpentUsd + trade.notionalUsd) <= policy.dailyMaxUsd',
      { trade: { dailySpentUsd: 98000, notionalUsd: 5000 }, policy: { dailyMaxUsd: 100000 } })).toBe(false);
  });

  // ── Faithfulness regression: bare top-level keys (the false-positive bug) ──
  // Constraints in the ledger reference bare `config`/`trade`/`policy`/`security`, not `context.*`.
  // The MCP handler spreads the context's own keys into scope so these resolve; mirror that here.

  it('faithfully evaluates a bare top-level === true constraint (was false-positive)', () => {
    const ctx = { config: { propagation: { atomic: true, requiresRestart: false, maxLatencyMs: 100 } } };
    const scope = { context: ctx, ...ctx };
    expect(simpleEval('config.propagation.atomic === true', scope)).toBe(true);
    expect(simpleEval('config.propagation.requiresRestart === false', scope)).toBe(true);
    expect(simpleEval('config.propagation.maxLatencyMs <= 500', scope)).toBe(true);
  });

  it('faithfully passes a fully-compliant pre-trade order (only the real cap should ever fail)', () => {
    const ctx = {
      trade: { notionalUsd: 500, dailySpentUsd: 0, symbol: 'AAPL', side: 'buy', assetClass: 'equity', accountType: 'agentic', userConfirmed: true },
      policy: { perTradeMaxUsd: 1000, dailyMaxUsd: 100000, tickerAllowlist: ['AAPL', 'MSFT'], allowedSides: ['buy', 'sell'], optionsAllowed: false, isolatedAccountOnly: true, confirmBeforeExecute: true },
    };
    const scope = { context: ctx, ...ctx };
    expect(simpleEval('trade.notionalUsd <= policy.perTradeMaxUsd', scope)).toBe(true);
    expect(simpleEval('(trade.dailySpentUsd + trade.notionalUsd) <= policy.dailyMaxUsd', scope)).toBe(true);
    expect(simpleEval('policy.tickerAllowlist.includes(trade.symbol)', scope)).toBe(true);
    expect(simpleEval('policy.allowedSides.includes(trade.side)', scope)).toBe(true);
    expect(simpleEval('policy.optionsAllowed === true || trade.assetClass !== \'option\'', scope)).toBe(true);
    expect(simpleEval('policy.isolatedAccountOnly === false || trade.accountType === \'agentic\'', scope)).toBe(true);
    expect(simpleEval('policy.confirmBeforeExecute === false || trade.userConfirmed === true', scope)).toBe(true);
  });

  it('still flags a genuine per-trade-cap violation (no false negative)', () => {
    const ctx = { trade: { notionalUsd: 5000 }, policy: { perTradeMaxUsd: 1000 } };
    const scope = { context: ctx, ...ctx };
    expect(simpleEval('trade.notionalUsd <= policy.perTradeMaxUsd', scope)).toBe(false);
  });
});
