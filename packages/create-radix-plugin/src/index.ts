// create-radix-plugin — canonical Radix plugin scaffolder (ADR-0024)
//
// Generates the ONE canonical plugin shape from ADR-0024 section 1:
//
//   plugins/<id>/
//     plugin.toml             # the single manifest (TOML) — section 2 schema
//     procedures/<id>.px      # ALL decision logic (compiles to PluresDB procedures)
//     adapter/<id>-adapter.ts # thin IO seam ONLY — the documented side-effect boundary
//     ui/Dashboard.svelte     # UI contribution, built on @plures/design-dojo
//     tests/<id>.test.ts      # plugin tests (section 6)
//     README.md
//
// The three-way split (ADR-0024 section 1) is enforced BY CONSTRUCTION:
//   - decision/inference/validation logic  -> procedures/*.px  (never TS/Svelte)
//   - side effects (network/fs/crypto/llm)  -> adapter/*.ts     (declared, gated)
//   - UI contribution                       -> ui/*.svelte on @plures/design-dojo
// The template REFUSES to emit raw HTML decision markup or TS decision logic:
// the generated .svelte is a thin design-dojo binding, the generated adapter is a
// documented IO seam with NO branching business logic, and every decision rule
// lives in the generated .px.
//
// Usage:
//   tsx src/index.ts <plugin-id> [--name "Human Name"] [--desc "..."] [--icon X]
//                                 [--out <plugins-dir>] [--force]
// Defaults: --out resolves to <repo>/plugins ; --name derives from id.

import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));

// ── argument parsing (no decision logic — pure IO/config seam) ───────────────
export interface Options {
  id: string;
  name: string;
  desc: string;
  icon: string;
  outDir: string;
  force: boolean;
}

export class ScaffoldError extends Error {}

const ID_RE = /^[a-z][a-z0-9-]*$/;

export function parseArgs(argv: string[]): Options {
  const positionals: string[] = [];
  const flags: Record<string, string | boolean> = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const key = a.slice(2);
      const next = argv[i + 1];
      if (next === undefined || next.startsWith("--")) {
        flags[key] = true;
      } else {
        flags[key] = next;
        i++;
      }
    } else {
      positionals.push(a);
    }
  }

  const id = positionals[0];
  if (!id) {
    throw new ScaffoldError(
      "missing <plugin-id>. usage: create-radix-plugin <plugin-id> [--name ..] [--desc ..] [--icon ..] [--out ..] [--force]",
    );
  }
  if (!ID_RE.test(id)) {
    throw new ScaffoldError(
      `invalid plugin id "${id}": must match ${ID_RE} (lowercase, kebab-case, start with a letter)`,
    );
  }

  const name =
    typeof flags.name === "string"
      ? flags.name
      : id
          .split("-")
          .map((s) => s.charAt(0).toUpperCase() + s.slice(1))
          .join(" ");

  const defaultOut = path.join(findRepoRoot(HERE), "plugins");

  return {
    id,
    name,
    desc:
      typeof flags.desc === "string"
        ? flags.desc
        : `${name} — canonical Radix plugin (.px logic, adapter IO seam, design-dojo UI)`,
    icon: typeof flags.icon === "string" ? flags.icon : "🧩",
    outDir: typeof flags.out === "string" ? flags.out : defaultOut,
    force: flags.force === true,
  };
}

function findRepoRoot(start: string): string {
  let dir = start;
  for (let i = 0; i < 12; i++) {
    if (fs.existsSync(path.join(dir, "pnpm-workspace.yaml"))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return path.resolve(start, "..", "..", "..");
}

// kebab-id -> camelCase identifier
function camel(id: string): string {
  return id.replace(/-([a-z0-9])/g, (_, c: string) => c.toUpperCase());
}

// ── template bodies (real content, ADR-0024-consistent) ──────────────────────

function pluginToml(o: Options): string {
  return `# ${o.name} — canonical Radix plugin (ADR-0024)
#
# The three-way split is mandatory and inspectable (ADR-0024 section 1):
#   decision/validation logic -> procedures/${o.id}.px  (compiles to PluresDB procedures)
#   side effects (IO)         -> adapter/${o.id}-adapter.ts  (the declared boundary)
#   UI contribution           -> ui/*.svelte on @plures/design-dojo
#   state                     -> PluresDB via ctx.data.collection()  (never localStorage)
# manifest.json is a GENERATED projection of THIS file (C-DRIFT-001) — never hand-edit it.

[plugin]
id = "${o.id}"
name = "${o.name}"
version = "0.1.0"
icon = "${o.icon}"
description = "${o.desc}"
trust = "local"                # verified | community | local (ADR-0011 section 5)

# ── Capabilities (ADR-0022) ──────────────────────────────────────────────────
[capabilities.required]
# provider capabilities (versioned interfaces) resolved by the loader; none by default
[capabilities.optional]
llm = "^1.0"                   # feature-detected; absent => degrade gracefully
[capabilities.provided]
# (none — this scaffold is a pure consumer. Add a CID here + capabilities/${o.id}.cid.toml
#  to become a provider; doing so flips the test gate to BLOCK, ADR-0024 section 6.)

# ── Platform permissions (ADR-0011) — closed, host-owned set ─────────────────
[permissions]
storage = true                 # PluresDB collections (state lives here, never localStorage)
network = false
llm = "budgeted"

# ── Plugin dependencies (VS Code-style, ADR-0024 section 3) ──────────────────
[dependencies]
plugins = []                   # hard plugin deps, loaded first (topo-sort)
capabilities = []              # e.g. ["secrets@^1.0"] — resolved to a provider, auto-installed

# ── UI contributions (host derives routes/nav/settings from these) ───────────
[[contributes.routes]]
path = "/"
component = "ui/Dashboard.svelte"
title = "Overview"

[[contributes.navItems]]
href = "/${o.id}/"
label = "${o.name}"
icon = "${o.icon}"

[[contributes.settings]]
key = "enabled"                # namespaced -> ${o.id}.enabled
type = "boolean"
default = true
label = "Enable ${o.name}"
`;
}

function procedurePx(o: Options): string {
  return `# ${o.id}.px — decision logic for the ${o.name} plugin (ADR-0024 section 1)
#
# ALL decision/validation/inference for this plugin lives here and compiles to a
# PluresDB procedure. NO branching business logic in the adapter or the UI; the
# adapter only performs the declared IO the "refresh" step names, the UI only
# renders the result node this procedure writes.
#
# Reactive contract (mediated, ADR-0011 — no direct function refs cross a seam):
#   consumer/UI emits  ${o.id}.refresh.requested { }        (a request)
#   this procedure     -> adapter IO at boundary -> writes ${o.id}:status node
#   this procedure emits ${o.id}.refresh.completed { ok, checked_at }

# ═══════════════════════════════════════════════════════════════════════
# Facts (documentation only — the shape this procedure reasons over)
# ═══════════════════════════════════════════════════════════════════════

# [parser-skip] fact Status:
# [parser-skip]   ok: boolean          # did the last refresh succeed
# [parser-skip]   checked_at: datetime # when it last ran

# ═══════════════════════════════════════════════════════════════════════
# Pure rule: classify an adapter result into a Status (total, no IO)
# ═══════════════════════════════════════════════════════════════════════

rule classify_status:
  given: "map a raw adapter probe result to a Status ok flag"
  when: $result.reachable == true
  then: {ok: true}
  else: {ok: false}

# ═══════════════════════════════════════════════════════════════════════
# Reactive procedure: service a refresh request
# ═══════════════════════════════════════════════════════════════════════

procedure on_refresh_requested:
  given: "React to ${o.id}.refresh.requested: probe via the adapter IO seam, classify, persist, and emit the result."
  trigger: "${o.id}.refresh.requested"

  # IO happens ONLY at the declared boundary (adapter/${o.id}-adapter.ts).
  call ${o.id}.adapter.probe {} -> $result   # boundary: io

  # Pure classification of the IO result.
  classify_status { result: $result } -> $status

  # State lives in PluresDB (never localStorage) — ctx.data collection node.
  write_node "${o.id}:status" {
    ok: $status.ok,
    checked_at: $now
  }

  emit "${o.id}.refresh.completed" {
    ok: $status.ok,
    checked_at: $now
  }

  return {ok: $status.ok}

# ═══════════════════════════════════════════════════════════════════════
# Expectations (invariants the plugin test proves)
# ═══════════════════════════════════════════════════════════════════════

# [parser-skip] expect status_total:
# [parser-skip]   given: "any adapter probe result"
# [parser-skip]   require: "classify_status.ok in {true, false}"
# [parser-skip]   because: "the classification is total — every probe classifies"

# [parser-skip] expect refresh_persists:
# [parser-skip]   given: "on_refresh_requested runs to completion"
# [parser-skip]   require: "a ${o.id}:status node exists with a checked_at timestamp"
# [parser-skip]   because: "state must land in PluresDB, not be lost in memory (C-PLURES-003)"
`;
}

function adapterTs(o: Options): string {
  const cls = camel(o.id);
  return `// ${o.id}-adapter — the IO boundary for the ${o.name} plugin (ADR-0024 section 1)
//
// -------------------------------------------------------------------------
// THIS FILE IS A THIN SIDE-EFFECT SEAM. IT CONTAINS NO DECISION LOGIC.
// -------------------------------------------------------------------------
// Every branch/inference/validation decision belongs in procedures/${o.id}.px
// (compiled to a PluresDB procedure). This adapter only PERFORMS the declared IO
// that a .px step names at a "boundary: io" call and returns a raw result for the
// .px to classify. It must not decide anything, must not write state directly
// (state lands in PluresDB via the host ctx.data seam the .px write_node compiles
// to), and must respect the [permissions] declared in plugin.toml (this scaffold
// declares storage = true, network = false).
//
// The single seam here — probe — is invoked by the .px procedure
// on_refresh_requested via the mediated call ${o.id}.adapter.probe. Replace its
// body with the real IO this plugin needs (a network fetch, an fs read, a crypto
// call, an LLM request) and declare the matching permission in plugin.toml. Keep
// it side-effect-only: probe, return raw data, let the .px decide.

/** Raw result of the probe IO. Shape is intentionally dumb: the .px classifies it. */
export interface ProbeResult {
  /** whether the probed dependency/resource was reachable */
  reachable: boolean;
  /** optional raw detail for diagnostics; never interpreted here */
  detail?: string;
}

/**
 * The IO seam. Performs the side effect ONLY and returns a raw result.
 *
 * Boundary contract:
 *  - no branching business logic (that is in ${o.id}.px classify_status)
 *  - no direct PluresDB writes (the .px write_node handles state)
 *  - honors plugin.toml [permissions]
 *
 * The default implementation is a real, honest reachability probe of the plugin's
 * own runtime (a truthful local check that requires no permission). It is NOT a
 * stub for missing logic (C-NOSTUB-001) — the decision logic genuinely lives in
 * the .px; this is the complete IO surface for the generated skeleton. Extend it
 * with the real dependency probe when you add one.
 */
export async function probe(): Promise<ProbeResult> {
  const reachable = typeof globalThis !== "undefined";
  return { reachable, detail: "${o.id} runtime reachable" };
}

/** The adapter surface bound to the .px mediated ${o.id}.adapter.* namespace. */
export const ${cls}Adapter = { probe } as const;
`;
}

function dashboardSvelte(o: Options): string {
  return `<script lang="ts">
  // Dashboard.svelte — UI contribution for the ${o.name} plugin (ADR-0024 section 5)
  //
  // A THIN design-dojo binding. It contains NO decision logic (that is in
  // procedures/${o.id}.px) and NO hand-rolled decision markup. It emits the
  // ${o.id}.refresh.requested event and renders the ${o.id}:status node the .px
  // procedure writes to PluresDB. State is read from the host ctx, never from
  // localStorage.
  import { Card, Button, StatusBadge } from "@plures/design-dojo/primitives";
  import type { PluginContext } from "@plures/design-dojo";

  let { ctx }: { ctx: PluginContext } = $props();

  // Reactive read of the PluresDB status node the .px maintains.
  const status = $derived(
    ctx.data.node<{ ok: boolean; checked_at: string }>("${o.id}:status"),
  );

  function refresh() {
    // Fire the request event; the .px procedure does the work + persists state.
    ctx.events.emit("${o.id}.refresh.requested", {});
  }
</script>

<Card title="${o.name}" icon="${o.icon}">
  <StatusBadge ok={status?.ok ?? false} label={status?.ok ? "Healthy" : "Unknown"} />
  {#if status?.checked_at}
    <p>Last checked: {status.checked_at}</p>
  {/if}
  <Button onclick={refresh}>Refresh</Button>
</Card>
`;
}

function testTs(o: Options): string {
  const cls = camel(o.id);
  return `// ${o.id}.test.ts — plugin tests (ADR-0024 section 6)
//
// Consumer-plugin tests: exercise the adapter IO seam and the pure classification
// invariant the .px classify_status rule declares. For a PROVIDER (a plugin with
// [capabilities.provided]) these tests BLOCK the build (section 6) and must load
// the plugin in a real radix host and drive it through ctx.data/events.
import { describe, it, expect } from "vitest";
import { probe, ${cls}Adapter } from "../adapter/${o.id}-adapter.js";

/** twin of the .px pure rule classify_status — kept in sync with ${o.id}.px */
function classifyStatus(result: { reachable: boolean }): { ok: boolean } {
  return { ok: result.reachable === true };
}

describe("${o.id} adapter IO seam", () => {
  it("probe() returns a raw reachability result and no decision", async () => {
    const r = await probe();
    expect(typeof r.reachable).toBe("boolean");
  });

  it("exposes the mediated adapter surface", () => {
    expect(${cls}Adapter).toHaveProperty("probe");
  });
});

describe("${o.id} classify_status (.px pure rule invariant)", () => {
  it("is total — every probe result classifies to a boolean ok", () => {
    for (const reachable of [true, false]) {
      const { ok } = classifyStatus({ reachable });
      expect(typeof ok).toBe("boolean");
      expect(ok).toBe(reachable);
    }
  });
});
`;
}

function readme(o: Options): string {
  return `# ${o.icon} ${o.name}

Canonical Radix plugin (ADR-0024). Generated by \`@plures/create-radix-plugin\`.

## The three-way split (mandatory, inspectable)

| Concern | Lives in | Never in |
|---------|----------|----------|
| Decision / validation / inference logic | \`procedures/${o.id}.px\` (-> PluresDB procedures) | TS, Svelte |
| Side effects (network, fs, crypto, LLM) | \`adapter/${o.id}-adapter.ts\` (declared, permission-gated) | \`.px\`, UI |
| State | PluresDB via \`ctx.data\` | \`localStorage\`, ad-hoc files |
| UI contribution | \`ui/*.svelte\` on \`@plures/design-dojo\` | \`.px\`, adapter |
| Cross-plugin interaction | mediated events / PluresDB nodes | direct function refs |

## Layout

\`\`\`
${o.id}/
  plugin.toml               # the single manifest (ADR-0024 section 2). manifest.json is GENERATED from this.
  procedures/${o.id}.px     # all decision logic
  adapter/${o.id}-adapter.ts# the IO seam only (no decisions)
  ui/Dashboard.svelte       # UI contribution (design-dojo)
  tests/${o.id}.test.ts     # tests (section 6)
  README.md
\`\`\`

## Reactive contract

1. UI (or a consumer) emits \`${o.id}.refresh.requested\`.
2. \`procedures/${o.id}.px\` (\`on_refresh_requested\`) calls the adapter probe at the IO
   boundary, classifies the result with the pure \`classify_status\` rule, writes a
   \`${o.id}:status\` PluresDB node, and emits \`${o.id}.refresh.completed\`.
3. \`ui/Dashboard.svelte\` renders the \`${o.id}:status\` node.

## Next steps

- Replace \`adapter/probe\` with the real IO your plugin needs; declare the matching
  \`[permissions]\` in \`plugin.toml\` (\`network\`, \`storage\`, \`llm\`, ...).
- Add real \`.px\` procedures/rules for your domain logic (keep IO out of them).
- To become a **provider**, add \`[capabilities.provided]\` + \`capabilities/${o.id}.cid.toml\`
  and real tests — the test gate then BLOCKS (ADR-0024 section 6).
- Never hand-edit \`manifest.json\`; it is generated from \`plugin.toml\` (C-DRIFT-001).

## References

- ADR-0024 — Canonical plugin format
- ADR-0022 — Capability host contract
- ADR-0011 — Plugin security
- \`plugins/commerce/\` — the reference provider plugin
`;
}

// ── generation (IO) ──────────────────────────────────────────────────────────

interface GenFile {
  rel: string;
  content: string;
}

function planFiles(o: Options): GenFile[] {
  return [
    { rel: "plugin.toml", content: pluginToml(o) },
    { rel: path.join("procedures", `${o.id}.px`), content: procedurePx(o) },
    { rel: path.join("adapter", `${o.id}-adapter.ts`), content: adapterTs(o) },
    { rel: path.join("ui", "Dashboard.svelte"), content: dashboardSvelte(o) },
    { rel: path.join("tests", `${o.id}.test.ts`), content: testTs(o) },
    { rel: "README.md", content: readme(o) },
  ];
}

export function scaffold(o: Options): { dir: string; files: string[] } {
  const dir = path.join(o.outDir, o.id);
  if (fs.existsSync(dir) && !o.force) {
    throw new ScaffoldError(
      `refusing to overwrite existing plugin dir: ${dir} (pass --force to replace)`,
    );
  }
  const files = planFiles(o);
  for (const f of files) {
    const abs = path.join(dir, f.rel);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, f.content, "utf8");
  }
  return { dir, files: files.map((f) => f.rel) };
}

// ── entrypoint ────────────────────────────────────────────────────────────────

function main(): void {
  let opts: Options;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (e) {
    if (e instanceof ScaffoldError) {
      console.error(`error: ${e.message}`);
      process.exit(2);
    }
    throw e;
  }

  try {
    const { dir, files } = scaffold(opts);
    console.log(`created canonical plugin: ${opts.id}`);
    console.log(`  dir: ${dir}`);
    for (const f of files) console.log(`  + ${f}`);
    console.log(
      `\nnext: implement adapter IO + .px logic, then add capabilities/${opts.id}.cid.toml to become a provider.`,
    );
  } catch (e) {
    if (e instanceof ScaffoldError) {
      console.error(`error: ${e.message}`);
      process.exit(1);
    }
    throw e;
  }
}

// run only when invoked as a CLI (not when imported by tests)
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
