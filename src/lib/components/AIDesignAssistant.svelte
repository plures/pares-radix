<script lang="ts">
	/**
	 * AIDesignAssistant — natural language → praxis schema generation
	 * Phase 4 of Design Mode.
	 */

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

	let prompt = $state('');
	let kind = $state<'auto' | 'page' | 'rule' | 'constraint' | 'widget'>('auto');
	let generating = $state(false);
	let result = $state<GenerationResult | null>(null);
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

<div class="ai-assistant">
	<header class="assistant-header">
		<h3>🤖 AI Design Assistant</h3>
		<p class="subtitle">Describe what you want to build in natural language</p>
	</header>

	<div class="input-area">
		<textarea
			bind:value={prompt}
			placeholder="e.g. 'Create a settings page with theme toggle, model selection, and API key input'"
			rows="3"
			class="prompt-input"
			onkeydown={(e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) handleGenerate(); }}
		></textarea>

		<div class="controls">
			<div class="kind-selector">
				{#each kindOptions as opt}
					<button
						class="kind-option"
						class:active={kind === opt.value}
						onclick={() => kind = opt.value as typeof kind}
						title={opt.description}
					>
						{opt.label}
					</button>
				{/each}
			</div>

			<button
				class="btn-generate"
				onclick={handleGenerate}
				disabled={!prompt.trim() || generating}
			>
				{generating ? '⏳ Generating...' : '✨ Generate'}
			</button>
		</div>
	</div>

	{#if error}
		<div class="error-banner">⚠️ {error}</div>
	{/if}

	{#if result}
		<div class="result-area">
			<div class="result-header">
				<span class="confidence" class:high={result.confidence >= 0.7} class:low={result.confidence < 0.5}>
					{Math.round(result.confidence * 100)}% confidence
				</span>
				{#if !llmComplete}
					<span class="fallback-badge">Template (no LLM)</span>
				{/if}
			</div>

			<p class="explanation">{result.explanation}</p>

			{#if result.schemas.length > 0}
				<div class="schemas-generated">
					<h4>Generated Schemas ({result.schemas.length})</h4>
					{#each result.schemas as schema}
						<div class="schema-preview">
							<span class="preview-kind">{schema.kind}</span>
							<span class="preview-id">{schema.id}</span>
							<span class="preview-desc">{schema.description}</span>
						</div>
					{/each}
					<button class="btn-apply" onclick={applySchemas}>
						🚀 Apply to Design Registry
					</button>
				</div>
			{/if}

			{#if result.layout}
				<div class="layout-generated">
					<h4>Generated Layout</h4>
					<pre class="layout-json">{JSON.stringify(result.layout, null, 2)}</pre>
				</div>
			{/if}

			{#if result.suggestions.length > 0}
				<div class="suggestions">
					<h4>💡 Suggestions</h4>
					<ul>
						{#each result.suggestions as suggestion}
							<li>{suggestion}</li>
						{/each}
					</ul>
				</div>
			{/if}
		</div>
	{/if}

	<footer class="assistant-footer">
		<kbd>Ctrl+Enter</kbd> to generate · Design-dojo components only · Praxis rules enforced
	</footer>
</div>

<style>
	.ai-assistant {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		padding: 1.5rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
	}

	.assistant-header h3 { margin: 0; font-size: 1.1rem; }
	.subtitle { margin: 0.25rem 0 0; font-size: 0.8rem; color: var(--color-text-muted); }

	.prompt-input {
		width: 100%;
		padding: 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-bg);
		color: var(--color-text);
		font-size: 0.9rem;
		resize: vertical;
		font-family: inherit;
	}
	.prompt-input:focus { outline: none; border-color: var(--color-accent); }

	.controls { display: flex; justify-content: space-between; align-items: center; gap: 0.5rem; }

	.kind-selector { display: flex; gap: 0.25rem; flex-wrap: wrap; }

	.kind-option {
		padding: 0.25rem 0.5rem;
		border: 1px solid var(--color-border);
		border-radius: 4px;
		background: transparent;
		color: var(--color-text-muted);
		cursor: pointer;
		font-size: 0.75rem;
	}
	.kind-option.active {
		background: var(--color-accent-bg);
		color: var(--color-accent);
		border-color: var(--color-accent);
	}

	.btn-generate {
		padding: 0.5rem 1.25rem;
		border: none;
		border-radius: 6px;
		background: var(--color-accent);
		color: white;
		cursor: pointer;
		font-weight: 500;
		white-space: nowrap;
	}
	.btn-generate:disabled { opacity: 0.5; cursor: not-allowed; }

	.error-banner {
		padding: 0.5rem 0.75rem;
		background: rgba(220, 38, 38, 0.1);
		border: 1px solid var(--color-danger);
		border-radius: 6px;
		color: var(--color-danger);
		font-size: 0.85rem;
	}

	.result-area {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 1rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
	}

	.result-header { display: flex; gap: 0.5rem; align-items: center; }

	.confidence {
		font-size: 0.75rem;
		padding: 0.15rem 0.5rem;
		border-radius: 4px;
		background: var(--color-hover);
	}
	.confidence.high { background: rgba(34, 197, 94, 0.15); color: #22c55e; }
	.confidence.low { background: rgba(245, 158, 11, 0.15); color: #f59e0b; }

	.fallback-badge {
		font-size: 0.65rem;
		padding: 0.1rem 0.35rem;
		border-radius: 3px;
		background: var(--color-hover);
		color: var(--color-text-muted);
	}

	.explanation { font-size: 0.85rem; color: var(--color-text-muted); margin: 0; }

	.schemas-generated h4, .layout-generated h4, .suggestions h4 {
		margin: 0 0 0.5rem; font-size: 0.85rem;
	}

	.schema-preview {
		display: flex; gap: 0.5rem; align-items: center;
		padding: 0.35rem 0.5rem; border: 1px solid var(--color-border);
		border-radius: 4px; margin-bottom: 0.25rem; font-size: 0.8rem;
	}
	.preview-kind {
		font-size: 0.65rem; padding: 0.1rem 0.35rem; border-radius: 3px;
		background: var(--color-accent-bg); color: var(--color-accent);
		text-transform: uppercase; font-weight: 600;
	}
	.preview-id { font-family: monospace; font-weight: 500; }
	.preview-desc { color: var(--color-text-muted); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.btn-apply {
		margin-top: 0.5rem; padding: 0.4rem 1rem; border: none;
		border-radius: 6px; background: #22c55e; color: white;
		cursor: pointer; font-weight: 500;
	}

	.layout-json {
		background: var(--color-bg); border: 1px solid var(--color-border);
		border-radius: 6px; padding: 0.75rem; font-size: 0.75rem;
		overflow-x: auto; max-height: 200px;
	}

	.suggestions ul {
		margin: 0; padding-left: 1.25rem; font-size: 0.8rem;
		color: var(--color-text-muted);
	}

	.assistant-footer {
		font-size: 0.7rem; color: var(--color-text-muted); text-align: center;
	}
	.assistant-footer kbd {
		padding: 0.1rem 0.3rem; border: 1px solid var(--color-border);
		border-radius: 3px; font-size: 0.65rem;
	}
</style>
