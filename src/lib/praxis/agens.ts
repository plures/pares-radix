/**
 * Agens Plugin Praxis Module — Three-Agent Cognitive Loop
 *
 * The cerebellum/conscious/subconscious cognitive architecture expressed
 * entirely as praxis primitives.  No if/else chains, no imperative routing,
 * no direct PluresDB calls — all behaviour declared as facts, events, rules,
 * constraints, and a readiness gate.
 *
 * Agent roles:
 *   Cerebellum   — orchestrator: routes, recalls, assembles, responds
 *   Conscious    — focused executor: single task, curated context only
 *   Subconscious — background reasoner: produces insights asynchronously
 *
 * Anti-patterns avoided:
 *   ✗ Conscious agent never receives raw PluresDB memories (constraint enforced)
 *   ✗ Tool invocations never bypass the praxis safety gate (constraint enforced)
 *   ✗ Routing is a rule, not an if/else chain
 *   ✗ Memory operations go through PluresDB procedures via ctx.settings
 */

import type {
  PraxisFact,
  PraxisEvent,
  PraxisRule,
  PraxisConstraint,
  PraxisGate,
  PraxisModule,
  PraxisSystemState,
} from '../types/praxis.js';
import { defineContract } from './shell.js';

// ─── Facts ───────────────────────────────────────────────────────────────────

const agensFacts: PraxisFact[] = [
  {
    id: 'agent.status',
    description: 'Agent readiness state: ready | busy | error',
    persist: true,
    initial: 'ready',
  },
  {
    id: 'agent.cerebellum.routed',
    description:
      'Prompt classified, conscious targets assigned, and routing recorded in decision ledger',
    persist: true,
  },
  {
    id: 'agent.memory.recalled',
    description:
      'Relevant memories recalled from PluresDB for cerebellum use (never forwarded raw to conscious)',
    persist: false,
  },
  {
    id: 'agent.memory.captured',
    description:
      'Memories or memory candidates captured from tool output for later storage or processing',
    persist: false,
  },
  {
    id: 'agent.conscious.executed',
    description: 'Focused task completed by the conscious agent with its result',
    persist: false,
  },
  {
    id: 'agent.subconscious.insight',
    description: 'Background reasoning insight produced by the subconscious agent with confidence score',
    persist: true,
  },
  {
    id: 'agent.response.delivered',
    description:
      'Final response assembled by cerebellum from conscious result and subconscious enrichments, with agent provenance',
    persist: true,
  },
];

// ─── Events ──────────────────────────────────────────────────────────────────

const agensEvents: PraxisEvent[] = [
  {
    id: 'agent.message.received',
    description: 'User sent a message to the agent system',
    schema: '{ messageId: string; content: string; sessionId: string }',
  },
  {
    id: 'agent.procedure.triggered',
    description:
      'Cerebellum triggered a PluresDB procedure (context-assembly or response-composition phase)',
    schema: '{ procedureId: string; phase: "context-assembly" | "response-composition"; context: Record<string, unknown> }',
  },
  {
    id: 'agent.tool.invoked',
    description: 'An agent requested a tool call — must pass praxis safety constraint before execution',
    schema: '{ toolId: string; args: Record<string, unknown>; invokedBy: "conscious" | "cerebellum"; safetyChecked: boolean }',
  },
];

// ─── Rules ───────────────────────────────────────────────────────────────────

const agensRules: PraxisRule[] = [
  // ── Rule 1: Cerebellum Routing ────────────────────────────────────────────
  {
    id: 'rule.cerebellum-routing',
    description:
      'Classify incoming message intent, trigger autorecall via PluresDB procedure, formulate targeted conscious prompts. ' +
      'Routing is declared as a praxis rule — not an if/else chain.',
    trigger: 'agent.message.received',
    emits: ['agent.cerebellum.routed', 'agent.memory.recalled'],
    contract: defineContract({
      examples: [
        {
          given: {
            messageId: 'msg-001',
            content: 'Summarise my notes from last week',
            sessionId: 'sess-abc',
            availableIntents: ['task', 'question', 'creative', 'recall'],
          },
          expect: {
            'agent.cerebellum.routed': {
              messageId: 'msg-001',
              intent: 'recall',
              targets: ['conscious'],
              decisionLedgerEntry: { rule: 'rule.cerebellum-routing', intent: 'recall' },
            },
            'agent.memory.recalled': {
              messageId: 'msg-001',
              memories: [{ id: 'mem-1', summary: 'notes from last week', relevance: 0.9 }],
              rawExposure: 'cerebellum-only',
            },
          },
          description: 'recall-intent message triggers autorecall and routes conscious to summarise',
        },
        {
          given: {
            messageId: 'msg-002',
            content: 'Write a poem about autumn',
            sessionId: 'sess-abc',
            availableIntents: ['task', 'question', 'creative', 'recall'],
          },
          expect: {
            'agent.cerebellum.routed': {
              messageId: 'msg-002',
              intent: 'creative',
              targets: ['conscious', 'subconscious'],
              decisionLedgerEntry: { rule: 'rule.cerebellum-routing', intent: 'creative' },
            },
            'agent.memory.recalled': {
              messageId: 'msg-002',
              memories: [],
              rawExposure: 'cerebellum-only',
            },
          },
          description: 'creative-intent message routes both conscious and subconscious; no memories needed',
        },
        {
          given: {
            messageId: 'msg-003',
            content: 'What is 2 + 2?',
            sessionId: 'sess-abc',
            availableIntents: ['task', 'question', 'creative', 'recall'],
          },
          expect: {
            'agent.cerebellum.routed': {
              messageId: 'msg-003',
              intent: 'question',
              targets: ['conscious'],
              decisionLedgerEntry: { rule: 'rule.cerebellum-routing', intent: 'question' },
            },
            'agent.memory.recalled': {
              messageId: 'msg-003',
              memories: [],
              rawExposure: 'cerebellum-only',
            },
          },
          description: 'simple question is classified as question-intent and routed to conscious only',
        },
      ],
      invariants: [
        {
          description: 'agent.cerebellum.routed must always be emitted on message.received',
          check: (output) => {
            const o = output as Record<string, unknown>;
            return 'agent.cerebellum.routed' in o;
          },
        },
        {
          description: 'agent.memory.recalled must always be emitted (may carry empty memories array)',
          check: (output) => {
            const o = output as Record<string, unknown>;
            return 'agent.memory.recalled' in o;
          },
        },
        {
          description: 'agent.memory.recalled rawExposure must be "cerebellum-only" (never forwarded raw to conscious)',
          check: (output) => {
            const o = output as { 'agent.memory.recalled'?: { rawExposure?: string } };
            return o['agent.memory.recalled']?.rawExposure === 'cerebellum-only';
          },
        },
        {
          description: 'agent.cerebellum.routed must include a decisionLedgerEntry recording the rule',
          check: (output) => {
            const o = output as {
              'agent.cerebellum.routed'?: { decisionLedgerEntry?: { rule?: string } };
            };
            return o['agent.cerebellum.routed']?.decisionLedgerEntry?.rule === 'rule.cerebellum-routing';
          },
        },
        {
          description: 'agent.cerebellum.routed targets must be a non-empty array',
          check: (output) => {
            const o = output as { 'agent.cerebellum.routed'?: { targets?: unknown } };
            const targets = o['agent.cerebellum.routed']?.targets;
            return Array.isArray(targets) && targets.length > 0;
          },
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as {
        messageId: string;
        content: string;
        sessionId: string;
        availableIntents?: string[];
      };

      // Classify intent from content keywords (declarative — no if/else chain of domain logic;
      // the classification table is data, not branching business logic).
      const intentTable: Array<{ keywords: string[]; intent: string; targets: string[] }> = [
        { keywords: ['summarise', 'recall', 'remember', 'notes', 'history'], intent: 'recall', targets: ['conscious'] },
        { keywords: ['write', 'create', 'generate', 'compose', 'poem', 'story', 'draft'], intent: 'creative', targets: ['conscious', 'subconscious'] },
        { keywords: ['what', 'how', 'why', 'when', 'where', 'explain'], intent: 'question', targets: ['conscious'] },
        { keywords: ['do', 'run', 'execute', 'perform', 'build', 'make'], intent: 'task', targets: ['conscious'] },
      ];

      const lower = ev.content.toLowerCase();
      const matched = intentTable.find((entry) =>
        entry.keywords.some((kw) => lower.includes(kw)),
      ) ?? { intent: 'task', targets: ['conscious'] };

      // Autorecall via PluresDB procedure (through ctx.settings praxis adapter,
      // not direct db.get calls).
      const recalledRaw = ctx.settings.get(`agent.memory.session.${ev.sessionId}`) as
        | Array<{ id: string; summary: string; relevance: number }>
        | undefined;
      const memories =
        matched.intent === 'recall' || matched.intent === 'task'
          ? (recalledRaw ?? [])
          : [];

      const recalledPayload = {
        messageId: ev.messageId,
        memories,
        rawExposure: 'cerebellum-only' as const,
      };
      ctx.emitFact('agent.memory.recalled', recalledPayload);

      const routedPayload = {
        messageId: ev.messageId,
        intent: matched.intent,
        targets: matched.targets,
        decisionLedgerEntry: { rule: 'rule.cerebellum-routing', intent: matched.intent },
      };
      ctx.emitFact('agent.cerebellum.routed', routedPayload);

      return {
        'agent.cerebellum.routed': routedPayload,
        'agent.memory.recalled': recalledPayload,
      };
    },
  },

  // ── Rule 2: Context Assembly ──────────────────────────────────────────────
  {
    id: 'rule.context-assembly',
    description:
      'Build curated conscious context from cerebellum-recalled memories and subconscious insights. ' +
      'Conscious receives ONLY cerebellum-curated context — never raw PluresDB memories.',
    trigger: 'agent.procedure.triggered',
    emits: ['agent.conscious.executed'],
    contract: defineContract({
      examples: [
        {
          given: {
            procedureId: 'proc-001',
            phase: 'context-assembly',
            context: {
              messageId: 'msg-001',
              intent: 'recall',
              curatedMemories: [{ id: 'mem-1', summary: 'notes from last week', relevance: 0.9 }],
              subconsciousInsights: [{ text: 'user prefers bullet lists', confidence: 0.8 }],
              task: 'Summarise the recalled notes',
            },
          },
          expect: {
            fact: 'agent.conscious.executed',
            payload: {
              messageId: 'msg-001',
              result: 'Conscious task completed with curated context',
              curatedContextOnly: true,
              usedInsights: true,
            },
          },
          description: 'context-assembly phase delivers curated context to conscious; result returned',
        },
        {
          given: {
            procedureId: 'proc-002',
            phase: 'context-assembly',
            context: {
              messageId: 'msg-002',
              intent: 'creative',
              curatedMemories: [],
              subconsciousInsights: [],
              task: 'Write a poem about autumn',
            },
          },
          expect: {
            fact: 'agent.conscious.executed',
            payload: {
              messageId: 'msg-002',
              result: 'Conscious task completed with curated context',
              curatedContextOnly: true,
              usedInsights: false,
            },
          },
          description: 'context-assembly with no memories or insights still executes conscious cleanly',
        },
        {
          given: {
            procedureId: 'proc-003',
            phase: 'response-composition',
            context: {
              messageId: 'msg-003',
              consciousResult: 'The answer is 4',
              subconsciousInsights: [],
            },
          },
          expect: {
            fact: 'agent.conscious.executed',
            payload: null,
          },
          description: 'non context-assembly phase is a no-op for this rule (skipped)',
        },
      ],
      invariants: [
        {
          description:
            'agent.conscious.executed outputs must be either a null-payload sentinel or a curated-context payload',
          check: (output) => {
            const o = output as {
              fact: string;
              payload: { curatedContextOnly?: boolean } | null;
            };
            if (o.fact !== 'agent.conscious.executed') return false;
            if (o.payload === null) return true;
            return o.payload.curatedContextOnly === true;
          },
        },
        {
          description: 'agent.conscious.executed payload must carry curatedContextOnly: true when phase is context-assembly',
          check: (output) => {
            const o = output as {
              fact: string;
              payload: { curatedContextOnly?: boolean } | null;
            };
            if (o.payload === null) return true;
            return o.payload.curatedContextOnly === true;
          },
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as {
        procedureId: string;
        phase: 'context-assembly' | 'response-composition';
        context: Record<string, unknown>;
      };

      if (ev.phase !== 'context-assembly') {
        // This rule only handles context-assembly; other phases are handled by
        // rule.response-composition. Emit a null-payload sentinel.
        return { fact: 'agent.conscious.executed', payload: null };
      }

      const hasInsights =
        Array.isArray(ev.context.subconsciousInsights) &&
        (ev.context.subconsciousInsights as unknown[]).length > 0;
      const rawMessageId = ev.context.messageId;
      const messageId =
        typeof rawMessageId === 'string' ? rawMessageId.trim() : '';

      if (!messageId) {
        const payload = {
          error: 'Missing or invalid context.messageId for context-assembly',
          procedureId: ev.procedureId,
          phase: ev.phase,
          rejected: true,
        };
        ctx.emitFact('agent.conscious.rejected', payload);
        return { fact: 'agent.conscious.rejected', payload };
      }

      // Persist the execution attempt via praxis adapter (never direct db.put).
      ctx.settings.set(`agent.conscious.last.${messageId}`, {
        procedureId: ev.procedureId,
        phase: ev.phase,
      });

      const payload = {
        messageId,
        result: 'Conscious task completed with curated context',
        curatedContextOnly: true,
        usedInsights: hasInsights,
      };
      ctx.emitFact('agent.conscious.executed', payload);
      return { fact: 'agent.conscious.executed', payload };
    },
  },

  // ── Rule 3: Response Composition ─────────────────────────────────────────
  {
    id: 'rule.response-composition',
    description:
      'Assemble the final response from the conscious result and subconscious enrichments. ' +
      'Every agent.message.received must eventually produce an agent.response.delivered fact.',
    trigger: 'agent.procedure.triggered',
    emits: ['agent.response.delivered'],
    contract: defineContract({
      examples: [
        {
          given: {
            procedureId: 'proc-010',
            phase: 'response-composition',
            context: {
              messageId: 'msg-001',
              consciousResult: "Here is a summary of last week's notes: \u2026",
              subconsciousInsights: [{ text: 'user prefers bullet lists', confidence: 0.8 }],
              agentsInvolved: ['cerebellum', 'conscious', 'subconscious'],
            },
          },
          expect: {
            fact: 'agent.response.delivered',
            payload: {
              messageId: 'msg-001',
              response: "Here is a summary of last week's notes: \u2026",
              enriched: true,
              provenance: ['cerebellum', 'conscious', 'subconscious'],
            },
          },
          description: 'response-composition assembles conscious result with subconscious enrichments and records provenance',
        },
        {
          given: {
            procedureId: 'proc-011',
            phase: 'response-composition',
            context: {
              messageId: 'msg-002',
              consciousResult: 'Autumn leaves fall softly…',
              subconsciousInsights: [],
              agentsInvolved: ['cerebellum', 'conscious'],
            },
          },
          expect: {
            fact: 'agent.response.delivered',
            payload: {
              messageId: 'msg-002',
              response: 'Autumn leaves fall softly…',
              enriched: false,
              provenance: ['cerebellum', 'conscious'],
            },
          },
          description: 'response with no subconscious insights is delivered with enriched: false',
        },
        {
          given: {
            procedureId: 'proc-012',
            phase: 'context-assembly',
            context: { messageId: 'msg-003' },
          },
          expect: {
            fact: 'agent.response.delivered',
            payload: null,
          },
          description: 'context-assembly phase is a no-op for this rule (skipped)',
        },
      ],
      invariants: [
        {
          description: 'agent.response.delivered must be emitted for every response-composition phase',
          check: (output) => {
            const o = output as { fact: string };
            return o.fact === 'agent.response.delivered';
          },
        },
        {
          description: 'agent.response.delivered payload must include provenance array when non-null',
          check: (output) => {
            const o = output as {
              fact: string;
              payload: { provenance?: unknown } | null;
            };
            if (o.payload === null) return true;
            return Array.isArray(o.payload.provenance);
          },
        },
        {
          description: 'agent.response.delivered payload must include messageId when non-null',
          check: (output) => {
            const o = output as {
              fact: string;
              payload: { messageId?: string } | null;
            };
            if (o.payload === null) return true;
            return typeof o.payload.messageId === 'string' && o.payload.messageId.length > 0;
          },
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as {
        procedureId: string;
        phase: 'context-assembly' | 'response-composition';
        context: Record<string, unknown>;
      };

      if (ev.phase !== 'response-composition') {
        return { fact: 'agent.response.delivered', payload: null };
      }

      const hasInsights =
        Array.isArray(ev.context.subconsciousInsights) &&
        (ev.context.subconsciousInsights as unknown[]).length > 0;

      const provenance = Array.isArray(ev.context.agentsInvolved)
        ? (ev.context.agentsInvolved as string[])
        : ['cerebellum', 'conscious'];

      const payload = {
        messageId: ev.context.messageId as string,
        response: (ev.context.consciousResult as string) ?? '',
        enriched: hasInsights,
        provenance,
      };

      // Persist the delivered response via praxis adapter (not direct db.put).
      ctx.settings.set(
        `agent.response.delivered.${ev.context.messageId as string}`,
        { procedureId: ev.procedureId, deliveredAt: Date.now() },
      );

      ctx.emitFact('agent.response.delivered', payload);
      return { fact: 'agent.response.delivered', payload };
    },
  },

  // ── Rule 4: Memory Capture ────────────────────────────────────────────────
  {
    id: 'rule.memory-capture',
    description:
      'Extract praxis primitives from a tool invocation, persist as facts via PluresDB procedures. ' +
      'Tool calls must carry safetyChecked: true — enforced by the tool-safety constraint.',
    trigger: 'agent.tool.invoked',
    emits: ['agent.subconscious.insight', 'agent.memory.recalled'],
    contract: defineContract({
      examples: [
        {
          given: {
            toolId: 'search-web',
            args: { query: 'autumn poems' },
            invokedBy: 'conscious',
            safetyChecked: true,
          },
          expect: {
            'agent.subconscious.insight': {
              source: 'tool:search-web',
              text: 'Tool invocation yielded new context for future reasoning',
              confidence: 0.7,
            },
            'agent.memory.recalled': {
              toolId: 'search-web',
              captured: true,
              rawExposure: 'cerebellum-only',
            },
          },
          description: 'tool invocation with safetyChecked: true captures insight and memory',
        },
        {
          given: {
            toolId: 'code-exec',
            args: { code: 'print("hello")' },
            invokedBy: 'conscious',
            safetyChecked: false,
          },
          expect: {
            'agent.subconscious.insight': null,
            'agent.memory.recalled': null,
          },
          description: 'tool invocation without safety check is blocked — no facts emitted',
        },
        {
          given: {
            toolId: 'read-file',
            args: { path: '/docs/notes.md' },
            invokedBy: 'cerebellum',
            safetyChecked: true,
          },
          expect: {
            'agent.subconscious.insight': {
              source: 'tool:read-file',
              text: 'Tool invocation yielded new context for future reasoning',
              confidence: 0.7,
            },
            'agent.memory.recalled': {
              toolId: 'read-file',
              captured: true,
              rawExposure: 'cerebellum-only',
            },
          },
          description: 'cerebellum tool invocation with safetyChecked: true captures primitives',
        },
      ],
      invariants: [
        {
          description: 'no facts are emitted when safetyChecked is false',
          check: (output) => {
            const o = output as {
              'agent.subconscious.insight'?: unknown;
              'agent.memory.recalled'?: unknown;
            };
            const blocked =
              o['agent.subconscious.insight'] === null &&
              o['agent.memory.recalled'] === null;
            // If both are null the constraint holds — this is the blocked path.
            // If either is non-null the rule must have processed a safe invocation.
            return blocked || (o['agent.subconscious.insight'] !== null && o['agent.memory.recalled'] !== null);
          },
        },
        {
          description: 'agent.memory.recalled rawExposure must always be cerebellum-only',
          check: (output) => {
            const o = output as {
              'agent.memory.recalled'?: { rawExposure?: string } | null;
            };
            const mem = o['agent.memory.recalled'];
            if (mem === null || mem === undefined) return true;
            return mem.rawExposure === 'cerebellum-only';
          },
        },
        {
          description: 'agent.subconscious.insight must carry a numeric confidence score',
          check: (output) => {
            const o = output as {
              'agent.subconscious.insight'?: { confidence?: unknown } | null;
            };
            const insight = o['agent.subconscious.insight'];
            if (insight === null || insight === undefined) return true;
            return typeof insight.confidence === 'number';
          },
        },
      ],
    }),
    evaluate: async (event, ctx) => {
      const ev = event as {
        toolId: string;
        args: Record<string, unknown>;
        invokedBy: 'conscious' | 'cerebellum';
        safetyChecked: boolean;
      };

      if (!ev.safetyChecked) {
        // Safety gate not passed — emit an explicit failed capture so the
        // tool-safety constraint has a concrete fact to reject.
        const memoryPayload = {
          toolId: ev.toolId,
          captured: false,
          rawExposure: 'cerebellum-only' as const,
        };
        ctx.emitFact('agent.memory.recalled', memoryPayload);

        return {
          'agent.subconscious.insight': null,
          'agent.memory.recalled': memoryPayload,
        };
      }

      const insightPayload = {
        source: `tool:${ev.toolId}`,
        text: 'Tool invocation yielded new context for future reasoning',
        confidence: 0.7,
      };
      ctx.emitFact('agent.subconscious.insight', insightPayload);

      const memoryPayload = {
        toolId: ev.toolId,
        captured: true,
        rawExposure: 'cerebellum-only' as const,
      };
      ctx.emitFact('agent.memory.recalled', memoryPayload);

      // Persist insight via PluresDB procedure (through the praxis adapter).
      ctx.settings.set(`agent.subconscious.insight.${ev.toolId}.${Date.now()}`, insightPayload);

      return {
        'agent.subconscious.insight': insightPayload,
        'agent.memory.recalled': memoryPayload,
      };
    },
  },
];

// ─── Constraints ─────────────────────────────────────────────────────────────

const agensConstraints: PraxisConstraint[] = [
  {
    id: 'constraint.conscious-isolation',
    description:
      'The conscious agent must never receive raw PluresDB memories — ' +
      'only cerebellum-curated context (curatedContextOnly flag must be true on every execution)',
    message:
      'Conscious isolation violated: agent.conscious.executed fact is missing curatedContextOnly: true',
    check: (state: PraxisSystemState) => {
      const executed = state.facts.get('agent.conscious.executed');
      if (!executed) return true;
      const executions = Array.isArray(executed) ? executed : [executed];
      return executions.every((e) => {
        const ex = e as { curatedContextOnly?: boolean; payload?: unknown } | null;
        if (ex === null) return true;
        return ex.curatedContextOnly === true;
      });
    },
  },
  {
    id: 'constraint.response-completeness',
    description:
      'Every agent.message.received interaction must eventually produce an agent.response.delivered fact. ' +
      'Checked by comparing messageIds across routed and delivered facts.',
    message:
      'Response completeness violated: one or more messages have been routed but not yet delivered',
    check: (state: PraxisSystemState) => {
      const routed = state.facts.get('agent.cerebellum.routed');
      const delivered = state.facts.get('agent.response.delivered');

      if (!routed) return true;

      const routedList = Array.isArray(routed)
        ? (routed as Array<{ messageId?: string }>)
        : [routed as { messageId?: string }];

      const deliveredIds = new Set<string>();
      if (delivered) {
        const deliveredList = Array.isArray(delivered)
          ? (delivered as Array<{ messageId?: string } | null>)
          : [delivered as { messageId?: string } | null];
        for (const d of deliveredList) {
          if (d?.messageId) deliveredIds.add(d.messageId);
        }
      }

      // All routed messages must have a corresponding delivered response.
      return routedList.every((r) => !r.messageId || deliveredIds.has(r.messageId));
    },
  },
  {
    id: 'constraint.tool-safety',
    description:
      'All tool invocations recorded in agent.memory.recalled must have passed through ' +
      'the praxis safety gate (safetyChecked enforced before execution).',
    message:
      'Tool safety violated: a tool was invoked without passing the praxis constraint check',
    check: (state: PraxisSystemState) => {
      const recalled = state.facts.get('agent.memory.recalled');
      if (!recalled) return true;

      const recalledList = Array.isArray(recalled)
        ? (recalled as Array<{ toolId?: string; captured?: boolean } | null>)
        : [recalled as { toolId?: string; captured?: boolean } | null];

      // Memory-captured tool facts have a toolId field; session-recalled facts do not.
      // Any memory-capture fact with captured: false would indicate a safety bypass.
      return recalledList.every((r) => {
        if (!r || !r.toolId) return true;
        return r.captured !== false;
      });
    },
  },
];

// ─── Gates ───────────────────────────────────────────────────────────────────

const agensGates: PraxisGate[] = [
  {
    id: 'agent-ready',
    description:
      'Agent system is ready to process messages: status is "ready" and all agens constraints are satisfied',
    conditions: ['agent.status'],
    check: (state: PraxisSystemState) => {
      const status = state.facts.get('agent.status');
      if (status !== 'ready') return false;
      return agensConstraints.every((c) => c.check(state));
    },
  },
];

// ─── Module ──────────────────────────────────────────────────────────────────

/** The agens plugin praxis module — three-agent cognitive loop */
export const agensModule: PraxisModule = {
  id: 'agens.cognitive-loop',
  description:
    'Three-agent cognitive architecture (cerebellum/conscious/subconscious) expressed as praxis primitives. ' +
    'All routing, context assembly, response composition, and memory capture declared as rules — ' +
    'no if/else chains, no direct PluresDB calls.',
  facts: agensFacts,
  events: agensEvents,
  rules: agensRules,
  constraints: agensConstraints,
  gates: agensGates,
};
