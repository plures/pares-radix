<!--
  CanvasRenderer — dynamically renders a CanvasDocument into a live app.

  This component:
  1. Walks the component tree from the CanvasDocument
  2. Resolves each node's type from the ComponentRegistry
  3. Binds props to PluresDB keys via the binding descriptors
  4. Wires procedures to user interactions
  5. Evaluates visibility conditions
  6. Recursively renders children

  The entire UI is data-driven. Change the PluresDB data, the UI updates.
  Change the component tree in PluresDB, the structure updates.
  No code generation. No compilation. Just data → UI.
-->
<script lang="ts">
  import { resolveComponent } from './registry.js';
  import { resolveUiTree } from './ui-resolve.js';
  import type { CanvasNode, CanvasDocument, CanvasBinding, CanvasCondition, CanvasProcedure, CanvasStep } from './format.js';

  // Props
  interface Props {
    /** The canvas document to render */
    document: CanvasDocument;
    /** PluresDB read function — (key) => current value */
    dbGet: (key: string) => unknown;
    /** PluresDB write function — (key, value, actor) => void */
    dbSet: (key: string, value: unknown) => void;
    /** PluresDB subscribe function — (key, callback) => unsubscribe */
    dbSubscribe?: (key: string, callback: (value: unknown) => void) => () => void;
    /** Canvas namespace prefix in PluresDB (default: "canvas:") */
    prefix?: string;
  }

  let { document, dbGet, dbSet, dbSubscribe, prefix = 'canvas:' }: Props = $props();

  // ── Reactive State Layer ──────────────────────────────────────────────────
  // Track subscribed values so Svelte re-renders when PluresDB changes.
  // This is what makes the canvas LIVE — AI writes → UI updates instantly.

  let subscribedValues = $state<Record<string, unknown>>({});
  let subscriptions: Array<() => void> = [];
  let renderVersion = $state(0); // bump to force re-render

  // ── Responsive viewport (Thread 2) ────────────────────────────────────────
  // The renderer renders the RESOLVED tree and re-renders when the viewport
  // changes, so every caller becomes responsive with ZERO caller changes.
  //
  // ui:viewport is a RAW key — it is NOT under the canvas: prefix. We subscribe
  // to it directly (bypassing `prefix`), in its own dedicated $effect, and do
  // NOT route it through collectBindingKeys (those are prefixed canvas bindings).
  let viewport = $state<{ width: number } | undefined>();

  $effect(() => {
    if (!dbSubscribe) return;
    // Seed from the current value so first render is already responsive.
    const seed = dbGet('ui:viewport');
    if (seed && typeof seed === 'object' && 'width' in (seed as Record<string, unknown>)) {
      viewport = seed as { width: number };
    }
    const unsub = dbSubscribe('ui:viewport', (value) => {
      viewport = (value && typeof value === 'object' && 'width' in (value as Record<string, unknown>))
        ? (value as { width: number })
        : undefined;
    });
    return unsub;
  });

  // The DERIVED tree: authored intent (document.tree) collapsed against the
  // active viewport breakpoint. resolveUiTree CLONES — document is never mutated.
  // With no viewport yet → identity clone → behavior is unchanged.
  const rendered = $derived(
    resolveUiTree(document.tree, viewport ? { viewport } : {}),
  );

  // Collect all binding keys from the document tree
  function collectBindingKeys(node: CanvasNode): string[] {
    const keys: string[] = [];
    if (node.bindings) {
      for (const binding of Object.values(node.bindings)) {
        const fullKey = binding.key.startsWith(prefix) ? binding.key : `${prefix}${binding.key}`;
        keys.push(fullKey);
      }
    }
    if (node.visible && typeof node.visible === 'object' && 'key' in node.visible) {
      const vk = node.visible.key;
      keys.push(vk.startsWith(prefix) ? vk : `${prefix}${vk}`);
    } else if (typeof node.visible === 'string') {
      keys.push(node.visible.startsWith(prefix) ? node.visible : `${prefix}${node.visible}`);
    }
    if (node.children) {
      for (const child of node.children) {
        keys.push(...collectBindingKeys(child));
      }
    }
    return keys;
  }

  // Subscribe to all binding keys when document changes
  $effect(() => {
    // Clean up previous subscriptions
    for (const unsub of subscriptions) unsub();
    subscriptions = [];

    if (!dbSubscribe) return;

    // Collect binding keys from the RESOLVED tree (props may differ post-resolve,
    // but bindings/visible keys are structural and survive the clone).
    const keys = [...new Set(collectBindingKeys(rendered))];
    for (const key of keys) {
      const unsub = dbSubscribe(key, (value) => {
        subscribedValues = { ...subscribedValues, [key]: value };
        renderVersion++;
      });
      subscriptions.push(unsub);
    }

    return () => {
      for (const unsub of subscriptions) unsub();
      subscriptions = [];
    };
  });

  // ── Binding Resolution ──────────────────────────────────────────────────

  function resolveBinding(binding: CanvasBinding): unknown {
    const fullKey = binding.key.startsWith(prefix) ? binding.key : `${prefix}${binding.key}`;
    // Use subscribed value if available (reactive), fall back to direct read
    let value = fullKey in subscribedValues ? subscribedValues[fullKey] : dbGet(fullKey);
    if (binding.readTransform) {
      // Simple transforms — extend as needed
      try {
        value = applyTransform(value, binding.readTransform);
      } catch { /* use raw value */ }
    }
    return value;
  }

  function writeBinding(binding: CanvasBinding, value: unknown): void {
    const fullKey = binding.key.startsWith(prefix) ? binding.key : `${prefix}${binding.key}`;
    let writeValue = value;
    if (binding.writeTransform) {
      try {
        writeValue = applyTransform(value, binding.writeTransform);
      } catch { /* use raw value */ }
    }
    dbSet(fullKey, writeValue);
  }

  function applyTransform(value: unknown, transform: string): unknown {
    // Basic transforms — the AI can use these in bindings
    switch (transform) {
      case 'toString': return String(value);
      case 'toNumber': return Number(value);
      case 'toBoolean': return Boolean(value);
      case 'not': return !value;
      case 'length': return Array.isArray(value) ? value.length : 0;
      case 'json': return JSON.stringify(value);
      case 'parse': return typeof value === 'string' ? JSON.parse(value) : value;
      default: return value;
    }
  }

  // ── Condition Evaluation ────────────────────────────────────────────────

  function evaluateCondition(cond: CanvasCondition | string): boolean {
    if (typeof cond === 'string') {
      // Simple key reference — truthy check
      const fullKey = cond.startsWith(prefix) ? cond : `${prefix}${cond}`;
      return Boolean(dbGet(fullKey));
    }

    const fullKey = cond.key.startsWith(prefix) ? cond.key : `${prefix}${cond.key}`;
    const value = dbGet(fullKey);

    switch (cond.op) {
      case 'truthy': return Boolean(value);
      case 'falsy': return !value;
      case 'eq': return value === cond.value;
      case 'neq': return value !== cond.value;
      case 'gt': return Number(value) > Number(cond.value);
      case 'lt': return Number(value) < Number(cond.value);
      case 'gte': return Number(value) >= Number(cond.value);
      case 'lte': return Number(value) <= Number(cond.value);
      case 'contains': return String(value).includes(String(cond.value));
      default: return true;
    }
  }

  // ── Procedure Execution ─────────────────────────────────────────────────

  function executeProcedure(proc: CanvasProcedure): void {
    for (const step of proc.steps) {
      executeStep(step);
    }
  }

  function executeStep(step: CanvasStep): void {
    const fullKey = step.key ? (step.key.startsWith(prefix) ? step.key : `${prefix}${step.key}`) : undefined;

    switch (step.kind) {
      case 'set':
        if (fullKey) dbSet(fullKey, resolveValue(step.value));
        break;
      case 'toggle':
        if (fullKey) dbSet(fullKey, !dbGet(fullKey));
        break;
      case 'increment':
        if (fullKey) dbSet(fullKey, Number(dbGet(fullKey) || 0) + Number(step.value ?? 1));
        break;
      case 'append':
        if (fullKey) {
          const arr = (dbGet(fullKey) as unknown[]) || [];
          dbSet(fullKey, [...arr, resolveValue(step.value)]);
        }
        break;
      case 'remove':
        if (fullKey) {
          const arr = (dbGet(fullKey) as unknown[]) || [];
          dbSet(fullKey, arr.filter((_, i) => i !== Number(step.value)));
        }
        break;
      case 'emit':
        // Emit an event that other procedures can listen to
        dbSet(`${prefix}_events:${step.value}`, { ts: Date.now() });
        break;
      case 'condition':
        if (step.condition && evaluateCondition(step.condition)) {
          step.then?.forEach(executeStep);
        } else {
          step.else?.forEach(executeStep);
        }
        break;
      case 'navigate':
        // Navigate to a different canvas or route
        dbSet(`${prefix}_navigate`, step.value);
        break;
    }
  }

  function resolveValue(value: unknown): unknown {
    if (typeof value !== 'string') return value;
    // Resolve ${key} references in string values
    return value.replace(/\$\{([^}]+)\}/g, (_match, key) => {
      const fullKey = key.startsWith(prefix) ? key : `${prefix}${key}`;
      return String(dbGet(fullKey) ?? '');
    });
  }

  // ── Node Resolution ─────────────────────────────────────────────────────

  function resolveProps(node: CanvasNode): Record<string, unknown> {
    const props: Record<string, unknown> = { ...(node.props || {}) };

    // Resolve bindings
    if (node.bindings) {
      for (const [propName, binding] of Object.entries(node.bindings)) {
        props[propName] = resolveBinding(binding);
      }
    }

    // Wire procedure triggers as event handlers
    for (const proc of document.procedures) {
      if (proc.trigger.nodeId === node.id) {
        switch (proc.trigger.kind) {
          case 'on_click':
            props['onclick'] = () => executeProcedure(proc);
            break;
          case 'on_change':
            props['onchange'] = () => executeProcedure(proc);
            break;
          case 'on_submit':
            props['onsubmit'] = () => executeProcedure(proc);
            break;
        }
      }
    }

    return props;
  }

  function isVisible(node: CanvasNode): boolean {
    // Honor the resolved `hidden` attribute: after resolveUiTree, a node hidden
    // at the active breakpoint carries a concrete props.hidden === true.
    if (node.props?.hidden === true) return false;
    if (node.visible === undefined) return true;
    return evaluateCondition(node.visible);
  }
</script>

{#snippet renderNode(node: CanvasNode)}
  {#if isVisible(node)}
    {@const meta = resolveComponent(node.type)}
    {#if meta}
      {@const props = resolveProps(node)}
      {#if meta.hasChildren && node.children?.length}
        <svelte:component this={meta.component} {...props}>
          {#each node.children as child (child.id)}
            {@render renderNode(child)}
          {/each}
        </svelte:component>
      {:else}
        <svelte:component this={meta.component} {...props} />
      {/if}
    {:else}
      <!-- Unknown component: {node.type} -->
    {/if}
  {/if}
{/snippet}

{@render renderNode(rendered)}
