# Task: close the loop — drive the responsive engine on the real /canvas surface

Architect: mswork. Base: 212a3dd. The UI schema+engine (resolve+validate+renderer) is shipped
and unit-green inside packages/canvas-runtime, but it is BUILT-BUT-UNWIRED in the running app:
no producer writes ui:viewport/ui:theme/ui:density, and no authored canvas carries responsive.*
intent. This task wires the loop so a human can SEE responsive reflow + theme reaction on /canvas.

Strategic objective: UI data in PluresDB → procedure auto-applies layout rules → what renders is
already correct for the current viewport/theme. This makes that observable on Radix's own surface.

Investigation already done (trust, but the file:lines are given — verify if you touch them):
- App canvas route: src/routes/canvas/+page.svelte — READ IT FIRST. It creates a reactive graph
  via createReactiveGraph(getSharedGraph()), subscribes canvas:_active, renders <CanvasRenderer
  document={activeCanvas} {dbGet} {dbSet} {dbSubscribe} prefix="canvas:" />. createNewCanvas()
  makes an EMPTY default tree ({id:'root',type:'PluginContentArea',children:[]}).
- Same pattern in src/lib/plugins/canvas/CanvasView.svelte (the plugin embed). Apply the SAME
  wiring to BOTH so the loop works whether reached via route or plugin pane.
- Existing theme system (REUSE, do not reinvent): src/lib/stores/praxis-svelte.svelte.ts emits
  fact 'theme.applied' as { value: 'light'|'dark' } (see its lines ~108-115, 57-73). The canvas
  resolver triggers on key 'ui:theme' shape { mode: 'light'|'dark' }. Bridge one to the other.
- attachViewportBridge is exported from @plures/canvas-runtime (def packages/canvas-runtime/src/
  ui-viewport-bridge.ts). It writes ui:viewport from window resize; browser-only; returns a detach.

## Deliverables (all REAL, no stubs/mocks — C-NOSTUB-001)

### 1. Produce ui:viewport (the IO edge)
In BOTH +page.svelte and CanvasView.svelte: on mount (browser only), call
`const detach = attachViewportBridge(graph)` and clean up on destroy (Svelte 5: `onMount(() =>
() => detach())` or `$effect(() => () => detach())`). Import attachViewportBridge from
'@plures/canvas-runtime'. This makes the renderer's ui:viewport subscription live.

### 2. Bridge the existing theme → ui:theme (prove theme practices honestly, reuse real state)
In BOTH mount points (or a shared helper if cleaner — your call, but keep it in src/ app layer):
subscribe to the existing 'theme.applied' fact on the shared graph and mirror it to 'ui:theme':
when theme.applied changes to { value }, `graph.put('ui:theme', { mode: value })`. Seed it once on
mount from the current value so it's correct before the first toggle. Do NOT create a second theme
toggle and do NOT invent density (no density app-state exists — leave ui:density unproduced and
say so; that's honest-absent, the density practices simply stay inert until a density control
exists). Verify the fact shape of theme.applied by READING praxis-svelte.svelte.ts before wiring —
if it's { value } use that; if it's a bare string, adapt. Match reality, don't assume.

### 3. Author ONE real responsive demo canvas (makes resolve VISIBLE)
This is the only thing that makes reflow observable. Add a real authored CanvasDocument that
exercises the actual practices, and a way to load it into canvas:_active. Two acceptable shapes —
pick the cleaner one:
  (a) a "Create Demo Canvas" Button next to "Create New Canvas" that builds the doc inline, OR
  (b) a seed module exported from src (e.g. src/lib/plugins/canvas/demo-canvas.ts) imported by both.
The document MUST use genuine responsive intent that the shipped resolver understands, e.g. a root
Box with children, where the root has:
  responsive: { direction: { base: 'column', md: 'row' }, gap: { base: '8px', md: '24px' },
                padding: { base: '8px', lg: '32px' } }
and at least one child Box with responsive: { hidden: { base: true, md: false } } (visible only at
md+), plus Text children using a themeToken (e.g. themeToken: 'fg' / 'accent') so the theme bridge
visibly recolors them on light/dark toggle. Use ONLY real component types from the registry (Box,
Text, Heading, Button) and ONLY real props (Box: direction/gap/padding/align/justify; Text: color/
size; NO background prop — there isn't one). Keep the tree small but real (a header row that stacks
on mobile, a sidebar box that hides on mobile, a couple of Text blocks). The point: resize narrow→
wide flips column→row, reveals the hidden box, widens gaps/padding; toggling theme recolors text.

### 4. Honest verification harness (channel-independent, C-TEST-002)
The visual proof is manual (resize the window), but you MUST add an automated test that proves the
WIRING produces the right RESOLVED tree — do not rely on "open the app and look". Add a test
(packages/canvas-runtime/tests/ OR src-level vitest if the app has one — prefer canvas-runtime since
that's where the suite lives) that:
  - takes the demo document's tree,
  - runs resolveUiTree(tree, { viewport:{ width: 375 } }) → asserts direction column, hidden child
    hidden, small gap; and resolveUiTree(tree, { viewport:{ width:1280 }, theme:{ mode:'dark' } }) →
    asserts direction row, child visible, wide gap, themed text color = THEME_TOKENS.fg.dark.
  - This guards the demo stays a faithful exercise of the engine (a living example, drift-guarded
    by behavior). Put the demo tree where the test can import it (shared module) so app + test use
    the SAME tree (single source of truth — C-DRIFT-001 spirit: the demo the user sees IS the demo
    the test verifies).

## GATE (must pass before reporting done)
- packages/canvas-runtime: ..\..\node_modules\.bin\vitest run  (FULL suite green; report counts)
- repo root C:\Projects\pares-radix: ..\..\Projects\pares-radix\node_modules\.bin\svelte-check --tsconfig .\tsconfig.json --threshold error  (report errors; the 1 pre-existing warning is fine; if your new .svelte/.ts adds errors, FIX them)
Keep pwsh SHORT, output BOUNDED (Select-Object -Last 12). Do NOT commit/push (architect integrates).
Do NOT modify the resolver/practices/CanvasRenderer engine internals — this is APP-LAYER wiring +
a demo + a behavior test only. If you find the engine genuinely can't express something the demo
needs, STOP and report it rather than hacking the engine.

## Report back
Files created/modified, the demo tree's shape, both gate results (vitest counts + svelte-check),
how you bridged theme (the exact theme.applied shape you found), and anything left honestly absent
(expected: ui:density has no producer — density control doesn't exist yet).