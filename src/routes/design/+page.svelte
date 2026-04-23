<script lang="ts">
	import { query, emitFact } from '$lib/stores/praxis-svelte.js';
	import type { DesignSchema, SchemaKind } from '$lib/praxis/design.js';
	import RuleEditor from '$lib/components/RuleEditor.svelte';
	import { applySchemaChange, recordDecision, getDecisionLedger } from '$lib/praxis/hot-reload.js';

	// Read schema registry from praxis facts
	let registry = $derived(
		(query<Record<string, DesignSchema>>('design.schema.registry')) ?? {}
	);

	let designModeActive = $derived(
		(query<{ active: boolean }>('design.mode.active')?.active) ?? false
	);

	let selectedKind = $state<SchemaKind | 'all'>('all');
	let selectedSchema = $state<string | null>(null);
	let searchQuery = $state('');
	let editingSchema = $state<string | null>(null);
	let showLedger = $state(false);

	let schemas = $derived(() => {
		let entries = Object.values(registry);
		if (selectedKind !== 'all') {
			entries = entries.filter(s => s.kind === selectedKind);
		}
		if (searchQuery) {
			const q = searchQuery.toLowerCase();
			entries = entries.filter(s =>
				s.id.toLowerCase().includes(q) ||
				s.description.toLowerCase().includes(q) ||
				s.moduleId.toLowerCase().includes(q)
			);
		}
		return entries.sort((a, b) => a.id.localeCompare(b.id));
	});

	let kindCounts = $derived(() => {
		const counts: Record<string, number> = { all: 0 };
		for (const schema of Object.values(registry)) {
			counts[schema.kind] = (counts[schema.kind] ?? 0) + 1;
			counts.all++;
		}
		return counts;
	});

	let selectedSchemaData = $derived(
		selectedSchema ? registry[selectedSchema] : null
	);

	const kindIcons: Record<SchemaKind | 'all', string> = {
		all: '📋',
		fact: '💡',
		event: '⚡',
		rule: '📐',
		constraint: '🔒',
		gate: '🚪',
		route: '🔗',
		component: '🧩',
	};

	function selectSchema(id: string) {
		selectedSchema = id;
		if (designModeActive) {
			emitFact('design.schema.selected', { schemaId: id });
		}
	}

	function startEditing(id: string) {
		editingSchema = id;
	}

	function handleSave(definition: Record<string, unknown>) {
		if (!editingSchema) return;
		const schema = registry[editingSchema];
		if (!schema) return;

		// Record the before state
		const before = { ...schema.definition };

		// Apply to live modules via hot-reload
		const updatedSchema = { ...schema, definition };
		const result = applySchemaChange(updatedSchema);

		// Record in decision ledger
		recordDecision({
			action: 'update',
			schemaId: editingSchema,
			schemaKind: schema.kind,
			before,
			after: definition,
			hotReloadResult: result,
		});

		// Persist via praxis event
		emitFact('design.schema.saved', { schemaId: editingSchema, definition });

		editingSchema = null;
	}

	function cancelEditing() {
		editingSchema = null;
		emitFact('design.schema.reverted', { schemaId: editingSchema });
	}

	let decisionLedger = $derived(getDecisionLedger());
</script>

<div class="design-page">
	<header class="design-header">
		<h1>🎨 Design Mode — Schema Explorer</h1>
		<p class="subtitle">
			{#if designModeActive}
				Editing enabled — select a schema to modify
			{:else}
				Read-only — enter design mode (Ctrl+Shift+D) to edit
			{/if}
		</p>
	</header>

	<div class="design-layout">
		<!-- Filter sidebar -->
		<aside class="filter-sidebar">
			<input
				type="search"
				placeholder="Search schemas..."
				bind:value={searchQuery}
				class="search-input"
			/>

			<nav class="kind-filters">
				{#each Object.entries(kindIcons) as [kind, icon]}
					<button
						class="kind-btn"
						class:active={selectedKind === kind}
						onclick={() => selectedKind = kind as SchemaKind | 'all'}
					>
						<span class="kind-icon">{icon}</span>
						<span class="kind-label">{kind}</span>
						<span class="kind-count">{kindCounts()[kind] ?? 0}</span>
					</button>
				{/each}
			</nav>

			<div class="schema-list">
				{#each schemas() as schema}
					<button
						class="schema-item"
						class:selected={selectedSchema === schema.id}
						onclick={() => selectSchema(schema.id)}
					>
						<span class="schema-icon">{kindIcons[schema.kind]}</span>
						<div class="schema-info">
							<span class="schema-id">{schema.id}</span>
							<span class="schema-module">{schema.moduleId}</span>
						</div>
						{#if schema.userCreated}
							<span class="user-badge">user</span>
						{/if}
					</button>
				{/each}
			</div>
		</aside>

	<!-- Detail panel -->
		<main class="detail-panel">
			{#if editingSchema && registry[editingSchema]}
				<RuleEditor
					schema={registry[editingSchema]}
					onSave={handleSave}
					onCancel={cancelEditing}
				/>
			{:else if selectedSchemaData}
				<div class="schema-detail">
					<div class="detail-header">
						<span class="detail-icon">{kindIcons[selectedSchemaData.kind]}</span>
						<div>
							<h2>{selectedSchemaData.id}</h2>
							<p class="detail-module">Module: {selectedSchemaData.moduleId}</p>
						</div>
						<span class="detail-kind-badge">{selectedSchemaData.kind}</span>
					</div>

					<p class="detail-description">{selectedSchemaData.description}</p>

					<section class="definition-section">
						<h3>Definition</h3>
						<pre class="definition-json">{JSON.stringify(selectedSchemaData.definition, null, 2)}</pre>
					</section>

					{#if designModeActive}
						<div class="edit-actions">
							<button class="btn-primary" onclick={() => startEditing(selectedSchemaData?.id ?? '')}>
								✏️ Edit Schema
							</button>
							{#if selectedSchemaData.userCreated}
								<button class="btn-danger" onclick={() => {
									emitFact('design.schema.deleted', { schemaId: selectedSchemaData?.id });
								}}>
									🗑️ Delete
								</button>
							{/if}
						</div>
					{/if}

					<footer class="detail-footer">
						<span>Last modified: {selectedSchemaData.updatedAt}</span>
						{#if selectedSchemaData.userCreated}
							<span>User-created</span>
						{:else}
							<span>Built-in</span>
						{/if}
					</footer>
				</div>
			{:else}
				<div class="empty-state">
					<span class="empty-icon">🎨</span>
					<h2>Select a schema</h2>
					<p>Choose a praxis primitive from the list to view its definition</p>
				</div>
			{/if}
		</main>
	</div>

	<!-- Decision Ledger -->
	{#if designModeActive}
		<footer class="ledger-bar">
			<button class="ledger-toggle" onclick={() => showLedger = !showLedger}>
				📜 Decision Ledger ({decisionLedger.length} entries)
				<span class="chevron">{showLedger ? '▲' : '▼'}</span>
			</button>
			{#if showLedger && decisionLedger.length > 0}
				<div class="ledger-entries">
					{#each decisionLedger.toReversed() as entry}
						<div class="ledger-entry">
							<span class="ledger-action">{entry.action}</span>
							<span class="ledger-schema">{entry.schemaId}</span>
							<span class="ledger-time">{new Date(entry.timestamp).toLocaleTimeString()}</span>
							{#if entry.hotReloadResult.applied}
								<span class="ledger-status ok">✅ applied</span>
							{:else}
								<span class="ledger-status err">❌ {entry.hotReloadResult.error}</span>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		</footer>
	{/if}
</div>

<style>
	.design-page {
		padding: 1.5rem;
		height: 100%;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.design-header {
		margin-bottom: 1rem;
	}

	.design-header h1 {
		font-size: 1.5rem;
		margin: 0;
	}

	.subtitle {
		color: var(--color-text-muted);
		margin: 0.25rem 0 0;
		font-size: 0.875rem;
	}

	.design-layout {
		display: flex;
		gap: 1rem;
		flex: 1;
		min-height: 0;
	}

	.filter-sidebar {
		width: 320px;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		overflow: hidden;
	}

	.search-input {
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface);
		color: var(--color-text);
		font-size: 0.875rem;
	}

	.kind-filters {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
	}

	.kind-btn {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.25rem 0.5rem;
		border: 1px solid var(--color-border);
		border-radius: 4px;
		background: var(--color-surface);
		color: var(--color-text-muted);
		cursor: pointer;
		font-size: 0.75rem;
	}

	.kind-btn.active {
		background: var(--color-accent-bg);
		color: var(--color-accent);
		border-color: var(--color-accent);
	}

	.kind-count {
		background: var(--color-hover);
		padding: 0 0.25rem;
		border-radius: 3px;
		font-size: 0.7rem;
	}

	.schema-list {
		flex: 1;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.schema-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--color-text);
		cursor: pointer;
		text-align: left;
		width: 100%;
	}

	.schema-item:hover { background: var(--color-hover); }
	.schema-item.selected { background: var(--color-accent-bg); }

	.schema-info {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.schema-id {
		font-size: 0.8rem;
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.schema-module {
		font-size: 0.7rem;
		color: var(--color-text-muted);
	}

	.user-badge {
		font-size: 0.65rem;
		padding: 1px 4px;
		border-radius: 3px;
		background: var(--color-accent-bg);
		color: var(--color-accent);
	}

	.detail-panel {
		flex: 1;
		overflow-y: auto;
	}

	.schema-detail {
		padding: 1rem;
	}

	.detail-header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}

	.detail-icon { font-size: 2rem; }

	.detail-header h2 {
		margin: 0;
		font-size: 1.25rem;
	}

	.detail-module {
		margin: 0;
		font-size: 0.8rem;
		color: var(--color-text-muted);
	}

	.detail-kind-badge {
		margin-left: auto;
		padding: 0.25rem 0.5rem;
		border-radius: 4px;
		background: var(--color-accent-bg);
		color: var(--color-accent);
		font-size: 0.75rem;
		font-weight: 500;
		text-transform: uppercase;
	}

	.detail-description {
		color: var(--color-text-muted);
		line-height: 1.5;
	}

	.definition-section h3 {
		font-size: 0.875rem;
		margin: 1.5rem 0 0.5rem;
		color: var(--color-text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.definition-json {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 1rem;
		font-family: 'JetBrains Mono', 'Fira Code', monospace;
		font-size: 0.8rem;
		overflow-x: auto;
		line-height: 1.5;
	}

	.edit-actions {
		display: flex;
		gap: 0.5rem;
		margin-top: 1.5rem;
	}

	.btn-primary {
		padding: 0.5rem 1rem;
		border: none;
		border-radius: 6px;
		background: var(--color-accent);
		color: white;
		cursor: pointer;
		font-weight: 500;
	}

	.btn-danger {
		padding: 0.5rem 1rem;
		border: 1px solid var(--color-danger);
		border-radius: 6px;
		background: transparent;
		color: var(--color-danger);
		cursor: pointer;
		font-weight: 500;
	}

	.detail-footer {
		display: flex;
		justify-content: space-between;
		margin-top: 2rem;
		padding-top: 1rem;
		border-top: 1px solid var(--color-border);
		font-size: 0.75rem;
		color: var(--color-text-muted);
	}

	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--color-text-muted);
	}

	.empty-icon { font-size: 3rem; margin-bottom: 1rem; }
	.empty-state h2 { margin: 0; }
	.empty-state p { margin: 0.5rem 0 0; }

	/* Decision Ledger */
	.ledger-bar {
		border-top: 1px solid var(--color-border);
		padding: 0.5rem 0;
	}

	.ledger-toggle {
		width: 100%;
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.5rem;
		border: none;
		background: transparent;
		color: var(--color-text-muted);
		cursor: pointer;
		font-size: 0.8rem;
	}

	.ledger-entries {
		max-height: 200px;
		overflow-y: auto;
		padding: 0.5rem;
	}

	.ledger-entry {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.25rem 0;
		font-size: 0.75rem;
		border-bottom: 1px solid var(--color-border);
	}

	.ledger-action {
		padding: 0.1rem 0.35rem;
		border-radius: 3px;
		background: var(--color-accent-bg);
		color: var(--color-accent);
		font-weight: 500;
		text-transform: uppercase;
		font-size: 0.65rem;
	}

	.ledger-schema {
		font-family: 'JetBrains Mono', monospace;
		flex: 1;
	}

	.ledger-time { color: var(--color-text-muted); }
	.ledger-status.ok { color: #22c55e; }
	.ledger-status.err { color: var(--color-danger); }
</style>
