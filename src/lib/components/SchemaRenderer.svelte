<script lang="ts">
	/**
	 * SchemaRenderer — renders UI from a schema definition.
	 *
	 * This is the core of the self-designing capability: given a schema
	 * (produced by design mode), render the appropriate design-dojo
	 * components dynamically.
	 *
	 * Phase 3: static mapping from schema → component.
	 * Phase 4: LLM-assisted layout generation from natural language.
	 */

	interface SchemaNode {
		/** design-dojo component name */
		component: string;
		/** Props to pass to the component */
		props: Record<string, unknown>;
		/** Child nodes */
		children: SchemaNode[];
		/** Slot name if this is a named slot */
		slot?: string;
	}

	interface Props {
		/** The schema tree to render */
		schema: SchemaNode;
		/** Whether to show edit affordances (design mode) */
		editable?: boolean;
		/** Callback when a node is selected for editing */
		onNodeSelect?: (node: SchemaNode, path: number[]) => void;
	}

	let { schema, editable = false, onNodeSelect }: Props = $props();

	// Map component names to rendered elements.
	// In a full implementation, these would dynamically import from design-dojo.
	// For Phase 3, we render semantic HTML with the component name as a data attribute.
	function handleClick(node: SchemaNode, path: number[]) {
		if (editable && onNodeSelect) {
			onNodeSelect(node, path);
		}
	}
</script>

<div
	class="schema-node"
	class:editable
	data-component={schema.component}
	role={editable ? 'button' : undefined}
	tabindex={editable ? 0 : undefined}
	onclick={() => handleClick(schema, [])}
	onkeydown={(e) => { if (e.key === 'Enter') handleClick(schema, []); }}
>
	{#if editable}
		<span class="node-label">{schema.component}</span>
	{/if}

	{#if schema.component === 'Box' || schema.component === 'Block'}
		<div class="rendered-box" style:flex-direction={String(schema.props.direction ?? 'column')} style:gap={String(schema.props.gap ?? '0.5rem')}>
			{#each schema.children as child, idx}
				<svelte:self
					schema={child}
					{editable}
					onNodeSelect={(node: SchemaNode, path: number[]) => onNodeSelect?.(node, [idx, ...path])}
				/>
			{/each}
		</div>
	{:else if schema.component === 'Card'}
		<div class="rendered-card">
			{#if schema.props.title}
				<h3 class="card-title">{schema.props.title}</h3>
			{/if}
			{#each schema.children as child, idx}
				<svelte:self
					schema={child}
					{editable}
					onNodeSelect={(node: SchemaNode, path: number[]) => onNodeSelect?.(node, [idx, ...path])}
				/>
			{/each}
		</div>
	{:else if schema.component === 'Text'}
		<p class="rendered-text" data-variant={schema.props.variant}>
			{schema.props.content ?? ''}
		</p>
	{:else if schema.component === 'Button'}
		<button class="rendered-button" disabled={Boolean(schema.props.disabled)}>
			{schema.props.label ?? 'Button'}
		</button>
	{:else if schema.component === 'Input'}
		<input
			class="rendered-input"
			type={String(schema.props.type ?? 'text')}
			placeholder={String(schema.props.placeholder ?? '')}
			value={String(schema.props.value ?? '')}
		/>
	{:else if schema.component === 'Badge'}
		<span class="rendered-badge" data-variant={schema.props.variant}>
			{schema.props.text ?? ''}
		</span>
	{:else if schema.component === 'ProgressBar'}
		<div class="rendered-progress">
			<div class="progress-fill" style:width="{Number(schema.props.value ?? 0)}%"></div>
		</div>
	{:else if schema.component === 'EmptyState'}
		<div class="rendered-empty">
			<span class="empty-icon">{schema.props.icon ?? '📭'}</span>
			<p>{schema.props.message ?? 'Nothing here yet'}</p>
		</div>
	{:else}
		<!-- Fallback: render children in a generic container -->
		<div class="rendered-generic">
			{#each schema.children as child, idx}
				<svelte:self
					schema={child}
					{editable}
					onNodeSelect={(node: SchemaNode, path: number[]) => onNodeSelect?.(node, [idx, ...path])}
				/>
			{/each}
		</div>
	{/if}
</div>

<style>
	.schema-node {
		position: relative;
	}

	.schema-node.editable {
		outline: 1px dashed transparent;
		transition: outline-color 0.15s;
		cursor: pointer;
	}

	.schema-node.editable:hover {
		outline-color: var(--color-accent);
	}

	.node-label {
		position: absolute;
		top: -0.75rem;
		left: 0.25rem;
		font-size: 0.6rem;
		padding: 0 0.25rem;
		background: var(--color-accent);
		color: white;
		border-radius: 2px;
		opacity: 0;
		pointer-events: none;
		transition: opacity 0.15s;
	}

	.schema-node.editable:hover > .node-label { opacity: 1; }

	.rendered-box { display: flex; }
	.rendered-card {
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 1rem;
	}
	.card-title { margin: 0 0 0.5rem; font-size: 0.95rem; }
	.rendered-text { margin: 0; }
	.rendered-text[data-variant="heading"] { font-size: 1.25rem; font-weight: 600; }
	.rendered-text[data-variant="caption"] { font-size: 0.8rem; color: var(--color-text-muted); }
	.rendered-text[data-variant="mono"] { font-family: monospace; }

	.rendered-button {
		padding: 0.4rem 0.75rem;
		border: 1px solid var(--color-accent);
		border-radius: 6px;
		background: var(--color-accent);
		color: white;
		cursor: pointer;
	}

	.rendered-input {
		padding: 0.4rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-bg);
		color: var(--color-text);
		width: 100%;
	}

	.rendered-badge {
		display: inline-block;
		padding: 0.15rem 0.4rem;
		border-radius: 4px;
		font-size: 0.75rem;
		background: var(--color-accent-bg);
		color: var(--color-accent);
	}

	.rendered-progress {
		height: 6px;
		background: var(--color-hover);
		border-radius: 3px;
		overflow: hidden;
	}
	.progress-fill {
		height: 100%;
		background: var(--color-accent);
		border-radius: 3px;
		transition: width 0.3s;
	}

	.rendered-empty {
		text-align: center;
		padding: 2rem;
		color: var(--color-text-muted);
	}
	.empty-icon { font-size: 2rem; display: block; margin-bottom: 0.5rem; }
</style>
