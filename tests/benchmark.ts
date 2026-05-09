/**
 * Migration Benchmark Suite — pares-radix vs OpenClaw
 *
 * Tests the same prompts against both systems and measures:
 * - Response quality (manual rating 1-5)
 * - Latency (time to first token, time to complete)
 * - Tool use accuracy (did it use the right tools?)
 * - Memory recall (does it remember prior context?)
 * - Error rate (crashes, timeouts, wrong outputs)
 *
 * Usage:
 *   RADIX_DEV=1 npx tsx benchmark.ts --target radix
 *   RADIX_DEV=1 npx tsx benchmark.ts --target openclaw
 *   RADIX_DEV=1 npx tsx benchmark.ts --compare results-radix.json results-openclaw.json
 */

interface BenchmarkCase {
  id: string;
  category: 'answer-quality' | 'coding' | 'tool-use' | 'memory' | 'proactive' | 'latency';
  prompt: string;
  expectedBehavior: string;
  requiresTools?: string[];
  requiresMemory?: boolean;
  maxLatencyMs?: number;
}

const BENCHMARK_CASES: BenchmarkCase[] = [
  // ── Answer Quality ──────────────────────────────────────────────────────
  {
    id: 'aq-1',
    category: 'answer-quality',
    prompt: 'Explain the difference between Praxis rules and Praxis constraints in the plures architecture.',
    expectedBehavior: 'Should explain rules (evaluate conditions → emit facts) vs constraints (block mutations that violate invariants). Should reference PluresDB as the runtime.',
  },
  {
    id: 'aq-2',
    category: 'answer-quality',
    prompt: 'What is the canvas runtime and how does it enable AI-created apps?',
    expectedBehavior: 'Should explain CanvasDocument format, component registry, PluresDB-backed state, reactive rendering. Should mention that AI writes data, not code.',
  },
  {
    id: 'aq-3',
    category: 'answer-quality',
    prompt: 'How does the 5-second rolling buffer in Chronos work?',
    expectedBehavior: 'Should explain: all writes go to ring buffer regardless of level, on error the window flushes to sink, zero disk cost in happy path.',
  },

  // ── Coding ──────────────────────────────────────────────────────────────
  {
    id: 'code-1',
    category: 'coding',
    prompt: 'Write a Svelte 5 component using only design-dojo primitives (Box, Text, Button, Input) that renders a simple counter with increment/decrement buttons.',
    expectedBehavior: 'Should produce valid Svelte 5 code using $state, design-dojo imports, no raw HTML.',
  },
  {
    id: 'code-2',
    category: 'coding',
    prompt: 'Write a Rust function that parses a DTMS XML datacenter file and returns a list of server hostnames.',
    expectedBehavior: 'Should use quick-xml or serde, handle the <Datacenter><Server ComputerName="..."> format.',
  },

  // ── Tool Use ────────────────────────────────────────────────────────────
  {
    id: 'tool-1',
    category: 'tool-use',
    prompt: 'Create a new canvas app called "Task Tracker" with a heading, an input field, and an add button.',
    expectedBehavior: 'Should use canvas.create, canvas.addNode (Heading, Input, Button), canvas.setData.',
    requiresTools: ['canvas.create', 'canvas.addNode', 'canvas.setData'],
  },
  {
    id: 'tool-2',
    category: 'tool-use',
    prompt: 'Search PluresDB for all keys starting with "config/"',
    expectedBehavior: 'Should use db.keys with prefix "config/".',
    requiresTools: ['db.keys'],
  },

  // ── Memory ──────────────────────────────────────────────────────────────
  {
    id: 'mem-1',
    category: 'memory',
    prompt: 'What is the plugin build order we decided on?',
    expectedBehavior: 'Should recall: 1. Vault, 2. Agent Console, 3. Editor, 4. Sprint Log, 5. Financial Advisor, 6. NetOps Toolkit.',
    requiresMemory: true,
  },
  {
    id: 'mem-2',
    category: 'memory',
    prompt: 'What ADO work item is the config management tool linked to?',
    expectedBehavior: 'Should recall #2777727 in the Dialtone Management repo.',
    requiresMemory: true,
  },

  // ── Latency ─────────────────────────────────────────────────────────────
  {
    id: 'lat-1',
    category: 'latency',
    prompt: 'What time is it?',
    expectedBehavior: 'Should respond quickly with the current time.',
    maxLatencyMs: 3000,
  },
  {
    id: 'lat-2',
    category: 'latency',
    prompt: 'List the files in the current directory.',
    expectedBehavior: 'Should execute a tool (ls/dir) and return results quickly.',
    maxLatencyMs: 5000,
    requiresTools: ['exec'],
  },
];

interface BenchmarkResult {
  caseId: string;
  target: 'radix' | 'openclaw';
  timestamp: string;
  latencyMs: number;
  response: string;
  toolsUsed: string[];
  error: string | null;
  // Manual ratings (filled in after review)
  qualityRating?: number; // 1-5
  accuracyRating?: number; // 1-5
  notes?: string;
}

// Export for use
export { BENCHMARK_CASES, type BenchmarkCase, type BenchmarkResult };

// ── CLI ───────────────────────────────────────────────────────────────────────

if (typeof process !== 'undefined' && process.argv[1]?.includes('benchmark')) {
  console.log(`\n📊 Pares-Radix Migration Benchmark Suite`);
  console.log(`   ${BENCHMARK_CASES.length} test cases across ${new Set(BENCHMARK_CASES.map(c => c.category)).size} categories\n`);

  for (const c of BENCHMARK_CASES) {
    console.log(`[${c.id}] ${c.category}`);
    console.log(`  Prompt: ${c.prompt.substring(0, 80)}...`);
    console.log(`  Expected: ${c.expectedBehavior.substring(0, 80)}...`);
    if (c.requiresTools) console.log(`  Tools: ${c.requiresTools.join(', ')}`);
    if (c.maxLatencyMs) console.log(`  Max latency: ${c.maxLatencyMs}ms`);
    console.log();
  }

  console.log(`\nTo run: configure a model provider in pares-radix settings, then:`);
  console.log(`  1. Send each prompt through the /chat route`);
  console.log(`  2. Record response, latency, tools used`);
  console.log(`  3. Rate quality 1-5`);
  console.log(`  4. Compare with OpenClaw results on same prompts`);
}
