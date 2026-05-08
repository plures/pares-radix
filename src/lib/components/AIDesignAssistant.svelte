<script lang="ts">
	/**
	 * AIDesignAssistant — natural language → praxis schema generation
	 * Phase 4 of Design Mode.
	 */

	import { Box, Heading, Text, Button, TextArea, CodeBlock, List, ListItem } from '@plures/design-dojo';
	import { emitFact } from '$lib/stores/praxis-svelte.js';
	import { generateSchema, type GenerationResult, type SchemaNode } from '$lib/praxis/llm-schema-gen.js';
	import { applySchemaChange, recordDecision } from '$lib/praxis/hot-reload.js';

	interface Props {
		/** LLM complete function from platform context */
		llmComplete?: (prompt: string) => Promise<string>;
		/** Callback when a layout is generated (for SchemaRenderer preview) */
		onLayoutGenerated?: (layout: SchemaNode) => void;
	}

	let { llmComplete, onLayoutGenerated }: Props = $props();

	// eslint-disable-next-line plures/no-raw-stores
	let prompt = $state('');
	// eslint-disable-next-line plures/no-raw-stores
	let kind = $state<'auto' | 'page' | 'rule' | 'constraint' | 'widget'>('auto');
	// eslint-disable-next-line plures/no-raw-stores
	let generating = $state(false);
	// eslint-disable-next-line plures/no-raw-stores
	let result = $state<GenerationResult | null>(null);
	// eslint-disable-next-line plures/no-raw-stores
	let error = $state<string | null>(null);

	async function handleGenerate() {
		if (!prompt.trim()) return;
		generating = true;
		error = null;
		result = null;

		try {
			result = await generateSchema(
				{ prompt: prompt.trim(), kind },
				llmComplete,
			);

			if (result.layout && onLayoutGenerated) {
				onLayoutGenerated(result.layout);
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Generation failed';
		} finally {
			generating = false;
		}
	}

	function applySchemas() {
		if (!result?.schemas.length) return;

		for (const schema of result.schemas) {
			const applyResult = applySchemaChange(schema);
			recordDecision({
				action: 'create',
				schemaId: schema.id,
				schemaKind: schema.kind,
				before: null,
				after: schema.definition,
				hotReloadResult: applyResult,
			});
			emitFact('design.schema.saved', {
				schemaId: schema.id,
				definition: schema.definition,
			});
		}
	}

	const kindOptions = [
		{ value: 'auto', label: '🔮 Auto-detect', description: 'Let AI decide the schema type' },
		{ value: 'page', label: '📄 Page', description: 'Full page layout with components' },
		{ value: 'widget', label: '🧩 Widget', description: 'Dashboard widget or card' },
		{ value: 'rule', label: '📐 Rule', description: 'Praxis rule with trigger and contract' },
		{ value: 'constraint', label: '🔒 Constraint', description: 'System invariant / validation' },
	];
</script>

<Box class="ai-assistant">
	<Box as="header" class="assistant-header">
		<Heading level={3}>🤖 AI Design Assistant</Heading>
		<Text as="p" class="subtitle">Describe what you want to build in natural language</Text>
	</Box>

	<Box class="input-area">
		<TextArea
			class="prompt-input"
			label="Prompt"
			bind:value={prompt}
			placeholder="e.g. 'Create a settings page with theme toggle, model selection, and API key input'"
			rows={3}
			onkeydown={(e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) handleGenerate(); }}
		/>

		<Box class="controls">
			<Box class="kind-selector">
				{#each kindOptions as opt}
					<Button
						class={kind === opt.value ? 'kind-option active' : 'kind-option'}
						onclick={() => kind = opt.value as typeof kind}
						title={opt.description}
					>
						{opt.label}
					</Button>
				{/each}
			</Box>

			<Button
				class="btn-generate"
				onclick={handleGenerate}
				disabled={!prompt.trim() || generating}
			>
				{generating ? '⏳ Generating...' : '✨ Generate'}
			</Button>
		</Box>
	</Box>

	{#if error}
		<Box class="error-banner">⚠️ {error}</Box>
	{/if}

	{#if result}
		<Box class="result-area">
			<Box class="result-header">
				<Text
					as="span"
					class={`confidence ${result.confidence >= 0.7 ? 'high' : result.confidence < 0.5 ? 'low' : ''}`}
				>
					{Math.round(result.confidence * 100)}% confidence
				</Text>
				{#if !llmComplete}
					<Text as="span" class="fallback-badge">Template (no LLM)</Text>
				{/if}
			</Box>

			<Text as="p" class="explanation">{result.explanation}</Text>

			{#if result.schemas.length > 0}
				<Box class="schemas-generated">
					<Heading level={4}>Generated Schemas ({result.schemas.length})</Heading>
					{#each result.schemas as schema}
						<Box class="schema-preview">
							<Text as="span" class="preview-kind">{schema.kind}</Text>
							<Text as="span" class="preview-id">{schema.id}</Text>
							<Text as="span" class="preview-desc">{schema.description}</Text>
						</Box>
					{/each}
					<Button class="btn-apply" onclick={applySchemas}>
						🚀 Apply to Design Registry
					</Button>
				</Box>
			{/if}

			{#if result.layout}
				<Box class="layout-generated">
					<Heading level={4}>Generated Layout</Heading>
					<CodeBlock class="layout-json" language="json">
						{JSON.stringify(result.layout, null, 2)}
					</CodeBlock>
				</Box>
			{/if}

			{#if result.suggestions.length > 0}
				<Box class="suggestions">
					<Heading level={4}>💡 Suggestions</Heading>
					<List>
						{#each result.suggestions as suggestion}
							<ListItem>{suggestion}</ListItem>
						{/each}
					</List>
				</Box>
			{/if}
		</Box>
	{/if}

	<Box as="footer" class="assistant-footer">
		<Text as="kbd">Ctrl+Enter</Text>
		<Text as="span"> to generate · Design-dojo components only · Praxis rules enforced</Text>
	</Box>
</Box>

<style>
	:global(.ai-assistant) {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		padding: 1.5rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
	}

	:global(.assistant-header h3) { margin: 0; font-size: 1.1rem; }
	:global(.subtitle) { margin: 0.25rem 0 0; font-size: 0.8rem; color: var(--color-text-muted); }

	:global(.prompt-input) {
		width: 100%;
	}

	:global(.prompt-input textarea) {
		padding: 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-bg);
		color: var(--color-text);
		font-size: 0.9rem;
		resize: vertical;
		font-family: inherit;
	}
	:global(.prompt-input textarea:focus) { outline: none; border-color: var(--color-accent); }

	:global(.controls) { display: flex; justify-content: space-between; align-items: center; gap: 0.5rem; }

	:global(.kind-selector) { display: flex; gap: 0.25rem; flex-wrap: wrap; }

	:global(.kind-option) {
		padding: 0.25rem 0.5rem;
		border: 1px solid var(--color-border);
		border-radius: 4px;
		background: transparent;
		color: var(--color-text-muted);
		cursor: pointer;
		font-size: 0.75rem;
	}
	:global(.kind-option.active) {
		background: var(--color-accent-bg);
		color: var(--color-accent);
		border-color: var(--color-accent);
	}

	:global(.btn-generate) {
		padding: 0.5rem 1.25rem;
		border: none;
		border-radius: 6px;
		background: var(--color-accent);
		color: white;
		cursor: pointer;
		font-weight: 500;
		white-space: nowrap;
	}
	:global(.btn-generate:disabled) { opacity: 0.5; cursor: not-allowed; }

	:global(.error-banner) {
		padding: 0.5rem 0.75rem;
		background: rgba(220, 38, 38, 0.1);
		border: 1px solid var(--color-danger);
		border-radius: 6px;
		color: var(--color-danger);
		font-size: 0.85rem;
	}

	:global(.result-area) {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 1rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
	}

	:global(.result-header) { display: flex; gap: 0.5rem; align-items: center; }

	:global(.confidence) {
		font-size: 0.75rem;
		padding: 0.15rem 0.5rem;
		border-radius: 4px;
		background: var(--color-hover);
	}
	:global(.confidence.high) { background: rgba(34, 197, 94, 0.15); color: #22c55e; }
	:global(.confidence.low) { background: rgba(245, 158, 11, 0.15); color: #f59e0b; }

	:global(.fallback-badge) {
		font-size: 0.65rem;
		padding: 0.1rem 0.35rem;
		border-radius: 3px;
		background: var(--color-hover);
		color: var(--color-text-muted);
	}

	:global(.explanation) { font-size: 0.85rem; color: var(--color-text-muted); margin: 0; }

	:global(.schemas-generated h4), :global(.layout-generated h4), :global(.suggestions h4) {
		margin: 0 0 0.5rem; font-size: 0.85rem;
	}

	:global(.schema-preview) {
		display: flex; gap: 0.5rem; align-items: center;
		padding: 0.35rem 0.5rem; border: 1px solid var(--color-border);
		border-radius: 4px; margin-bottom: 0.25rem; font-size: 0.8rem;
	}
	:global(.preview-kind) {
		font-size: 0.65rem; padding: 0.1rem 0.35rem; border-radius: 3px;
		background: var(--color-accent-bg); color: var(--color-accent);
		text-transform: uppercase; font-weight: 600;
	}
	:global(.preview-id) { font-family: monospace; font-weight: 500; }
	:global(.preview-desc) { color: var(--color-text-muted); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	:global(.btn-apply) {
		margin-top: 0.5rem; padding: 0.4rem 1rem; border: none;
		border-radius: 6px; background: #22c55e; color: white;
		cursor: pointer; font-weight: 500;
	}

	:global(.layout-json) {
		background: var(--color-bg); border: 1px solid var(--color-border);
		border-radius: 6px; padding: 0.75rem; font-size: 0.75rem;
		overflow-x: auto; max-height: 200px;
	}

	:global(.suggestions ul) {
		margin: 0; padding-left: 1.25rem; font-size: 0.8rem;
		color: var(--color-text-muted);
	}

	:global(.assistant-footer) {
		font-size: 0.7rem; color: var(--color-text-muted); text-align: center;
		display: flex; align-items: center; justify-content: center; gap: 0.25rem;
	}
	:global(.assistant-footer kbd) {
		padding: 0.1rem 0.3rem; border: 1px solid var(--color-border);
		border-radius: 3px; font-size: 0.65rem;
	}
</style>
