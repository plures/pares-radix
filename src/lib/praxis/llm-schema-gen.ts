/**
 * LLM Schema Generator — Phase 4 of Design Mode
 *
 * Generates praxis-compliant schemas from natural language descriptions.
 * Uses the platform's LLM API to translate user intent into:
 * - Praxis rules with triggers, emits, and contracts
 * - UI layouts using design-dojo components
 * - Data requirements and constraints
 * - Navigation structures
 *
 * The generator produces SchemaNode trees that the SchemaRenderer can
 * render immediately, and DesignSchema objects for the praxis registry.
 */

import type { DesignSchema, SchemaKind } from './design.js';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface SchemaNode {
  component: string;
  props: Record<string, unknown>;
  children: SchemaNode[];
  slot?: string;
}

export interface GenerationRequest {
  /** Natural language description of what to build */
  prompt: string;
  /** What kind of schema to generate */
  kind: 'page' | 'rule' | 'constraint' | 'widget' | 'auto';
  /** Existing schemas for context (avoid duplicates, reference existing facts) */
  existingSchemas?: string[];
  /** Available design-dojo components */
  availableComponents?: string[];
}

export interface GenerationResult {
  /** Generated schema(s) */
  schemas: DesignSchema[];
  /** Generated UI layout (for page/widget kinds) */
  layout?: SchemaNode;
  /** Explanation of what was generated */
  explanation: string;
  /** Confidence score (0-1) */
  confidence: number;
  /** Suggested next steps */
  suggestions: string[];
}

// ─── Component Catalog (for LLM context) ────────────────────────────────────

const COMPONENT_CATALOG = `
Available design-dojo components:

LAYOUT: Box (direction, gap), Card (title), Sidebar (items, currentPath), 
StatusBar, TitleBar (title), Tabs (items, activeIndex), SplitPane (direction),
DashboardGrid (columns), ActivityBar (items), PluginContentArea

PRIMITIVES: Button (variant, onclick, disabled, label), Input (value, placeholder, type),
Toggle (checked, onchange), Select (options, value), SearchInput (value, results),
Text (variant: body|heading|caption|mono, content), MarkdownEditor (value, mode)

DATA: Table (columns, rows), List (items), TreeView (nodes)

OVERLAYS: CommandPalette (commands, open), Dialog (open, title), Toast (message, variant)

SURFACES: Card (title), ChatPane (messages)

FEEDBACK: ProgressBar (value, max), Badge (variant, text), EmptyState (icon, message)
`;

const PRAXIS_CONTEXT = `
Praxis primitives:

FACT: Named state with optional persistence. { id, description, persist }
EVENT: Trigger that drives rule evaluation. { id, description, schema }
RULE: Event→Fact transformation with contract. { id, description, trigger, emits, contract: { examples, invariants } }
CONSTRAINT: System invariant that must hold. { id, description, message, check }
GATE: Readiness guard. { id, description, conditions, check }

Rules MUST have contracts with examples (given→expect) and invariants (check functions).
Every decision goes through praxis rules, not bare if/else.
`;

// ─── Prompt Templates ────────────────────────────────────────────────────────

function buildPagePrompt(request: GenerationRequest): string {
  return `You are a UI designer for a praxis-first application.
Generate a page layout using ONLY these design-dojo components.

${COMPONENT_CATALOG}

User request: "${request.prompt}"

Respond with a JSON object:
{
  "layout": {
    "component": "Box",
    "props": { "direction": "column", "gap": "1rem" },
    "children": [...]
  },
  "explanation": "what this page does",
  "confidence": 0.85,
  "suggestions": ["next steps"]
}

Rules:
- Use ONLY components from the catalog above
- Nest components logically (Box for layout, Card for sections)
- Include realistic props and placeholder content
- Every page needs at least a heading and primary content area`;
}

function buildRulePrompt(request: GenerationRequest): string {
  return `You are a praxis rule designer.

${PRAXIS_CONTEXT}

Existing schemas in this app: ${request.existingSchemas?.join(', ') || 'none'}

User request: "${request.prompt}"

Respond with a JSON object:
{
  "schemas": [{
    "id": "rule.my-rule",
    "kind": "rule",
    "moduleId": "radix.user",
    "label": "My Rule",
    "description": "what this rule does",
    "definition": {
      "id": "rule.my-rule",
      "description": "...",
      "trigger": "some.event",
      "emits": ["some.fact"],
      "contractExamples": 2,
      "contractInvariants": 1
    },
    "userCreated": true,
    "updatedAt": "${new Date().toISOString()}"
  }],
  "explanation": "what this rule does and why",
  "confidence": 0.8,
  "suggestions": ["related rules to consider"]
}

Rules:
- Every rule MUST have a trigger event and at least one emitted fact
- Use existing facts/events where possible
- Include contract example and invariant counts
- Description must explain the business logic`;
}

function buildConstraintPrompt(request: GenerationRequest): string {
  return `You are a praxis constraint designer.

${PRAXIS_CONTEXT}

Existing schemas: ${request.existingSchemas?.join(', ') || 'none'}

User request: "${request.prompt}"

Respond with a JSON object:
{
  "schemas": [{
    "id": "constraint.my-constraint",
    "kind": "constraint",
    "moduleId": "radix.user",
    "label": "My Constraint",
    "description": "what this constraint enforces",
    "definition": {
      "id": "constraint.my-constraint",
      "description": "...",
      "message": "violation message shown when constraint fails"
    },
    "userCreated": true,
    "updatedAt": "${new Date().toISOString()}"
  }],
  "explanation": "what invariant this enforces",
  "confidence": 0.85,
  "suggestions": ["related constraints"]
}`;
}

// ─── Generator ───────────────────────────────────────────────────────────────

/**
 * Generate schemas from a natural language prompt.
 *
 * Uses the platform's LLM API. Falls back to template-based generation
 * if no LLM is available.
 */
export async function generateSchema(
  request: GenerationRequest,
  llmComplete?: (prompt: string) => Promise<string>,
): Promise<GenerationResult> {
  const kind = request.kind === 'auto' ? detectKind(request.prompt) : request.kind;

  // If no LLM available, use template-based fallback
  if (!llmComplete) {
    return templateFallback(request, kind);
  }

  const prompt = kind === 'page' || kind === 'widget'
    ? buildPagePrompt(request)
    : kind === 'rule'
    ? buildRulePrompt(request)
    : buildConstraintPrompt(request);

  try {
    const response = await llmComplete(prompt);
    const parsed = JSON.parse(extractJson(response));

    return {
      schemas: parsed.schemas ?? [],
      layout: parsed.layout ?? undefined,
      explanation: parsed.explanation ?? 'Generated from natural language',
      confidence: parsed.confidence ?? 0.7,
      suggestions: parsed.suggestions ?? [],
    };
  } catch {
    // LLM failed — fall back to templates
    return templateFallback(request, kind);
  }
}

// ─── Kind Detection ──────────────────────────────────────────────────────────

function detectKind(prompt: string): 'page' | 'rule' | 'constraint' | 'widget' {
  const lower = prompt.toLowerCase();

  if (lower.includes('page') || lower.includes('dashboard') || lower.includes('screen') || lower.includes('view')) {
    return 'page';
  }
  if (lower.includes('widget') || lower.includes('card') || lower.includes('panel')) {
    return 'widget';
  }
  if (lower.includes('constraint') || lower.includes('must not') || lower.includes('never') || lower.includes('always')) {
    return 'constraint';
  }
  if (lower.includes('when') || lower.includes('rule') || lower.includes('trigger') || lower.includes('if')) {
    return 'rule';
  }

  return 'page'; // default to page
}

// ─── JSON Extraction ─────────────────────────────────────────────────────────

function extractJson(text: string): string {
  // Try to find JSON in markdown code blocks
  const codeBlock = text.match(/```(?:json)?\s*\n?([\s\S]*?)```/);
  if (codeBlock) return codeBlock[1].trim();

  // Try to find raw JSON object
  const jsonMatch = text.match(/\{[\s\S]*\}/);
  if (jsonMatch) return jsonMatch[0];

  return text;
}

// ─── Template Fallback ───────────────────────────────────────────────────────

function templateFallback(
  request: GenerationRequest,
  kind: 'page' | 'rule' | 'constraint' | 'widget',
): GenerationResult {
  const timestamp = new Date().toISOString();
  const slug = request.prompt.toLowerCase().replace(/[^a-z0-9]+/g, '-').slice(0, 30);

  switch (kind) {
    case 'page':
      return {
        schemas: [],
        layout: {
          component: 'Box',
          props: { direction: 'column', gap: '1rem' },
          children: [
            {
              component: 'Text',
              props: { variant: 'heading', content: request.prompt },
              children: [],
            },
            {
              component: 'Card',
              props: { title: 'Content' },
              children: [
                {
                  component: 'Text',
                  props: { variant: 'body', content: `This page was generated from: "${request.prompt}"` },
                  children: [],
                },
                {
                  component: 'EmptyState',
                  props: { icon: '🎨', message: 'Add components from the Component Picker' },
                  children: [],
                },
              ],
            },
          ],
        },
        explanation: `Template page for "${request.prompt}". Add components from the Component Picker to build it out.`,
        confidence: 0.4,
        suggestions: [
          'Open the Component Picker to add more components',
          'Use the Route Editor to configure navigation',
          'Consider adding data requirements for this page',
        ],
      };

    case 'widget':
      return {
        schemas: [],
        layout: {
          component: 'Card',
          props: { title: request.prompt },
          children: [
            {
              component: 'Text',
              props: { variant: 'caption', content: 'Widget content goes here' },
              children: [],
            },
          ],
        },
        explanation: `Template widget for "${request.prompt}".`,
        confidence: 0.4,
        suggestions: ['Customize the widget content', 'Add data bindings'],
      };

    case 'rule':
      return {
        schemas: [{
          id: `rule.${slug}`,
          kind: 'rule' as SchemaKind,
          moduleId: 'radix.user',
          label: request.prompt,
          description: request.prompt,
          definition: {
            id: `rule.${slug}`,
            description: request.prompt,
            trigger: 'app.booted',
            emits: [`${slug}.result`],
            contractExamples: 0,
            contractInvariants: 0,
          },
          userCreated: true,
          updatedAt: timestamp,
        }],
        explanation: `Template rule for "${request.prompt}". Update the trigger event and add contract examples.`,
        confidence: 0.3,
        suggestions: [
          'Change the trigger to the appropriate event',
          'Add contract examples to define expected behavior',
          'Add invariants to enforce correctness',
        ],
      };

    case 'constraint':
      return {
        schemas: [{
          id: `constraint.${slug}`,
          kind: 'constraint' as SchemaKind,
          moduleId: 'radix.user',
          label: request.prompt,
          description: request.prompt,
          definition: {
            id: `constraint.${slug}`,
            description: request.prompt,
            message: `Constraint violated: ${request.prompt}`,
          },
          userCreated: true,
          updatedAt: timestamp,
        }],
        explanation: `Template constraint for "${request.prompt}". The check function needs implementation.`,
        confidence: 0.3,
        suggestions: [
          'Define the validation logic in the constraint check function',
          'Consider related constraints that should also exist',
        ],
      };
  }
}
