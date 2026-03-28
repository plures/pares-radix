/**
 * Praxis Inference Engine
 *
 * Runs registered inference rules against data records, produces
 * confidence-scored inferences with full decision chains.
 *
 * Pattern: immutable data → rules with confidence → LLM only where needed.
 */

import type {
  Inference,
  InferenceRule,
  InferenceInput,
  InferenceResult,
  DecisionEntry,
  InferenceAPI,
  CollectionAPI,
} from '../types/plugin.js';
import { getAllInferenceRules } from './plugin-loader.js';

/** Confidence threshold for auto-confirmation */
const AUTO_CONFIRM_THRESHOLD = 0.90;
/** Confidence threshold below which we ask the user */
const USER_GATE_THRESHOLD = 0.70;

/**
 * Create an InferenceAPI backed by a PluresDB collection.
 */
export function createInferenceEngine(
  inferenceCollection: CollectionAPI,
  decisionCollection: CollectionAPI,
): InferenceAPI {
  return {
    async infer(sourceType: string, record: Record<string, unknown>): Promise<Inference[]> {
      const rules = getAllInferenceRules().filter(r => r.appliesTo.includes(sourceType));
      if (rules.length === 0) return [];

      const sourceId = (record.id as string) ?? crypto.randomUUID();

      // Get historical data for context
      const history = await inferenceCollection.query({ sourceType });
      const priorInferences = await inferenceCollection.query({ sourceId }) as Inference[];
      const confirmedInferences = (await inferenceCollection.query({
        sourceType,
        confirmed: true,
      })) as Inference[];

      const input: InferenceInput = {
        record,
        history: history as Record<string, unknown>[],
        priorInferences,
        confirmedInferences,
      };

      const inferences: Inference[] = [];

      for (const rule of rules) {
        try {
          const result = rule.evaluate(input);
          if (!result) continue;

          const inference = buildInference(sourceId, sourceType, rule, result);
          await inferenceCollection.put(inference.id, inference);

          // Record the decision
          const decision = buildDecision(inference.id, rule, input, result);
          await decisionCollection.put(decision.timestamp, decision);

          inferences.push(inference);
        } catch (err) {
          console.error(`[radix:inference] Rule "${rule.id}" failed:`, err);
        }
      }

      // Merge inferences for the same field — compound confidence
      return mergeInferences(inferences);
    },

    async getInferences(sourceId: string): Promise<Inference[]> {
      return (await inferenceCollection.query({ sourceId })) as Inference[];
    },

    async confirm(inferenceId: string, confirmed: boolean): Promise<void> {
      const existing = (await inferenceCollection.get(inferenceId)) as Inference | null;
      if (!existing) return;

      await inferenceCollection.put(inferenceId, {
        ...existing,
        confirmed,
        confirmedBy: 'user',
        updatedAt: new Date().toISOString(),
      });
    },

    async getDecisionChain(inferenceId: string): Promise<DecisionEntry[]> {
      const decisions = (await decisionCollection.query({ inferenceId })) as DecisionEntry[];
      return decisions.sort((a, b) => a.timestamp.localeCompare(b.timestamp));
    },
  };
}

// ─── Helpers ────────────────────────────────────────────────────────────────

function buildInference(
  sourceId: string,
  sourceType: string,
  rule: InferenceRule,
  result: InferenceResult,
): Inference {
  const now = new Date().toISOString();
  const confidence = Math.min(1.0, result.confidence);

  return {
    id: crypto.randomUUID(),
    sourceId,
    sourceType,
    field: result.field,
    value: result.value,
    confidence,
    strategy: rule.id,
    decisionChain: [],
    confirmed: confidence >= AUTO_CONFIRM_THRESHOLD,
    confirmedBy: confidence >= AUTO_CONFIRM_THRESHOLD ? 'auto' : undefined,
    createdAt: now,
    updatedAt: now,
  };
}

function buildDecision(
  inferenceId: string,
  rule: InferenceRule,
  input: InferenceInput,
  result: InferenceResult,
): DecisionEntry {
  return {
    ruleId: rule.id,
    input: { recordId: input.record.id, fieldCount: Object.keys(input.record).length },
    output: result.value,
    confidenceDelta: result.confidence - rule.baseConfidence,
    reasoning: result.reasoning,
    timestamp: new Date().toISOString(),
  };
}

/**
 * When multiple rules fire for the same field on the same record,
 * compound their confidence scores rather than keeping duplicates.
 */
function mergeInferences(inferences: Inference[]): Inference[] {
  const byField = new Map<string, Inference[]>();

  for (const inf of inferences) {
    const key = `${inf.sourceId}:${inf.field}:${JSON.stringify(inf.value)}`;
    if (!byField.has(key)) byField.set(key, []);
    byField.get(key)!.push(inf);
  }

  const merged: Inference[] = [];
  for (const [, group] of byField) {
    if (group.length === 1) {
      merged.push(group[0]);
      continue;
    }

    // Compound: 1 - ∏(1 - confidence_i)
    const compound = 1 - group.reduce((acc, inf) => acc * (1 - inf.confidence), 1);
    const primary = group.reduce((a, b) => (a.confidence > b.confidence ? a : b));

    merged.push({
      ...primary,
      confidence: Math.min(1.0, compound),
      strategy: group.map(g => g.strategy).join('+'),
      confirmed: compound >= AUTO_CONFIRM_THRESHOLD,
      confirmedBy: compound >= AUTO_CONFIRM_THRESHOLD ? 'auto' : undefined,
    });
  }

  return merged;
}

/**
 * Check if an inference needs user confirmation.
 */
export function needsUserConfirmation(inference: Inference): boolean {
  return !inference.confirmed && inference.confidence < USER_GATE_THRESHOLD;
}

/**
 * Check if an inference was auto-confirmed.
 */
export function isAutoConfirmed(inference: Inference): boolean {
  return inference.confirmed && inference.confirmedBy === 'auto';
}
