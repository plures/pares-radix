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

	import { Box, Text, Heading, Button, Input } from '@plures/design-dojo';
	import SchemaRenderer from './SchemaRenderer.svelte';

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

{#if editable}
	<Box
		class="schema-node editable"
		data-component={schema.component}
		role="button"
		tabindex={0}
		onclick={() => handleClick(schema, [])}
		
	>
		<Text as="span" class="node-label">{schema.component}</Text>

		{#if schema.component === 'Box' || schema.component === 'Block'}
			<Box
				class="rendered-box"
				direction={schema.props.direction === 'row' ? 'row' : 'column'}
				gap={String(schema.props.gap ?? '0.5rem')}
			>
				{#each schema.children as child, idx}
					<SchemaRenderer
						schema={child}
						{editable}
						onNodeSelect={(node: SchemaNode, path: number[]) => onNodeSelect?.(node, [idx, ...path])}
					/>
				{/each}
			</Box>
		{:else if schema.component === 'Card'}
			<Box class="rendered-card">
				{#if schema.props.title}
					<Heading level={3} class="card-title">{schema.props.title}</Heading>
				{/if}
				{#each schema.children as child, idx}
					<SchemaRenderer
						schema={child}
						{editable}
						onNodeSelect={(node: SchemaNode, path: number[]) => onNodeSelect?.(node, [idx, ...path])}
					/>
				{/each}
			</Box>
		{:else if schema.component === 'Text'}
			<Text as="p" class="rendered-text" data-variant={schema.props.variant ? String(schema.props.variant) : undefined}>
				{schema.props.content ?? ''}
			</Text>
		{:else if schema.component === 'Button'}
			<Button class="rendered-button" disabled={Boolean(schema.props.disabled)}>
				{schema.props.label ?? 'Button'}
			</Button>
		{:else if schema.component === 'Input'}
			<Input
				class="rendered-input"
				type={String(schema.props.type ?? 'text')}
				placeholder={String(schema.props.placeholder ?? '')}
				value={String(schema.props.value ?? '')}
			/>
		{:else if schema.component === 'Badge'}
			<Text as="span" class="rendered-badge" data-variant={schema.props.variant ? String(schema.props.variant) : undefined}>
				{schema.props.text ?? ''}
			</Text>
		{:else if schema.component === 'ProgressBar'}
			<Text as="p" class="rendered-progress" data-value={schema.props.value !== undefined ? String(schema.props.value) : undefined}>
				Progress: {Number(schema.props.value ?? 0)}%
			</Text>
		{:else if schema.component === 'EmptyState'}
			<Box class="rendered-empty">
				<Text as="span" class="empty-icon">{schema.props.icon ?? '📭'}</Text>
				<Text as="p">{schema.props.message ?? 'Nothing here yet'}</Text>
			</Box>
		{:else}
			<!-- Fallback: render children in a generic container -->
			<Box class="rendered-generic">
				{#each schema.children as child, idx}
					<SchemaRenderer
						schema={child}
						{editable}
						onNodeSelect={(node: SchemaNode, path: number[]) => onNodeSelect?.(node, [idx, ...path])}
					/>
				{/each}
			</Box>
		{/if}
	</Box>
{:else}
	<Box
		class="schema-node"
		data-component={schema.component}
	>
		{#if schema.component === 'Box' || schema.component === 'Block'}
			<Box
				class="rendered-box"
				direction={schema.props.direction === 'row' ? 'row' : 'column'}
				gap={String(schema.props.gap ?? '0.5rem')}
			>
				{#each schema.children as child, idx}
					<SchemaRenderer
						schema={child}
						{editable}
						onNodeSelect={(node: SchemaNode, path: number[]) => onNodeSelect?.(node, [idx, ...path])}
					/>
				{/each}
			</Box>
		{:else if schema.component === 'Card'}
			<Box class="rendered-card">
				{#if schema.props.title}
					<Heading level={3} class="card-title">{schema.props.title}</Heading>
				{/if}
				{#each schema.children as child, idx}
					<SchemaRenderer
						schema={child}
						{editable}
						onNodeSelect={(node: SchemaNode, path: number[]) => onNodeSelect?.(node, [idx, ...path])}
					/>
				{/each}
			</Box>
		{:else if schema.component === 'Text'}
			<Text as="p" class="rendered-text" data-variant={schema.props.variant ? String(schema.props.variant) : undefined}>
				{schema.props.content ?? ''}
			</Text>
		{:else if schema.component === 'Button'}
			<Button class="rendered-button" disabled={Boolean(schema.props.disabled)}>
				{schema.props.label ?? 'Button'}
			</Button>
		{:else if schema.component === 'Input'}
			<Input
				class="rendered-input"
				type={String(schema.props.type ?? 'text')}
				placeholder={String(schema.props.placeholder ?? '')}
				value={String(schema.props.value ?? '')}
			/>
		{:else if schema.component === 'Badge'}
			<Text as="span" class="rendered-badge" data-variant={schema.props.variant ? String(schema.props.variant) : undefined}>
				{schema.props.text ?? ''}
			</Text>
		{:else if schema.component === 'ProgressBar'}
			<Text as="p" class="rendered-progress" data-value={schema.props.value !== undefined ? String(schema.props.value) : undefined}>
				Progress: {Number(schema.props.value ?? 0)}%
			</Text>
		{:else if schema.component === 'EmptyState'}
			<Box class="rendered-empty">
				<Text as="span" class="empty-icon">{schema.props.icon ?? '📭'}</Text>
				<Text as="p">{schema.props.message ?? 'Nothing here yet'}</Text>
			</Box>
		{:else}
			<!-- Fallback: render children in a generic container -->
			<Box class="rendered-generic">
				{#each schema.children as child, idx}
					<SchemaRenderer
						schema={child}
						{editable}
						onNodeSelect={(node: SchemaNode, path: number[]) => onNodeSelect?.(node, [idx, ...path])}
					/>
				{/each}
			</Box>
		{/if}
	</Box>
{/if}

<style>
	:global(.schema-node) {
		position: relative;
	}

	:global(.schema-node.editable) {
		outline: 1px dashed transparent;
		transition: outline-color 0.15s;
		cursor: pointer;
	}

	:global(.schema-node.editable:hover) {
		outline-color: var(--color-accent);
	}

	:global(.node-label) {
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

	:global(.schema-node.editable:hover > .node-label) { opacity: 1; }

	:global(.rendered-box) { display: flex; }
	:global(.rendered-card) {
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 1rem;
	}
	:global(.card-title) { margin: 0 0 0.5rem; font-size: 0.95rem; }
	:global(.rendered-text) { margin: 0; }
	:global(.rendered-text[data-variant="heading"]) { font-size: 1.25rem; font-weight: 600; }
	:global(.rendered-text[data-variant="caption"]) { font-size: 0.8rem; color: var(--color-text-muted); }
	:global(.rendered-text[data-variant="mono"]) { font-family: monospace; }

	:global(.rendered-button) {
		padding: 0.4rem 0.75rem;
		border: 1px solid var(--color-accent);
		border-radius: 6px;
		background: var(--color-accent);
		color: white;
		cursor: pointer;
	}

	:global(.rendered-input) {
		padding: 0.4rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-bg);
		color: var(--color-text);
		width: 100%;
	}

	:global(.rendered-badge) {
		display: inline-block;
		padding: 0.15rem 0.4rem;
		border-radius: 4px;
		font-size: 0.75rem;
		background: var(--color-accent-bg);
		color: var(--color-accent);
	}

	:global(.rendered-progress) {
		margin: 0;
		font-size: 0.8rem;
		color: var(--color-text-muted);
	}

	:global(.rendered-empty) {
		text-align: center;
		padding: 2rem;
		color: var(--color-text-muted);
	}
	:global(.empty-icon) { font-size: 2rem; display: block; margin-bottom: 0.5rem; }
</style>
