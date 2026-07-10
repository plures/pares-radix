<script lang="ts">
	import { query, emitFact } from '$lib/stores/praxis-svelte.svelte.js';
	import type { DesignSchema, SchemaKind } from '$lib/praxis/design.js';
	import RuleEditor from '$lib/components/RuleEditor.svelte';
	import { Box, Heading, Text, Input, Button, Badge, CodeBlock } from '@plures/design-dojo';
	import { applySchemaChange, recordDecision, getDecisionLedger } from '$lib/praxis/hot-reload.js';

	// Read schema registry from praxis facts
	// eslint-disable-next-line plures/no-raw-stores
	let registry = $derived(
		(query<Record<string, DesignSchema>>('design.schema.registry')) ?? {}
	);

	// eslint-disable-next-line plures/no-raw-stores
	let designModeActive = $derived(
		(query<{ active: boolean }>('design.mode.active')?.active) ?? false
	);

	// eslint-disable-next-line plures/no-raw-stores
	let selectedKind = $state<SchemaKind | 'all'>('all');
	// eslint-disable-next-line plures/no-raw-stores
	let selectedSchema = $state<string | null>(null);
	// eslint-disable-next-line plures/no-raw-stores
	let searchQuery = $state('');
	// eslint-disable-next-line plures/no-raw-stores
	let editingSchema = $state<string | null>(null);
	// eslint-disable-next-line plures/no-raw-stores
	let showLedger = $state(false);

	// eslint-disable-next-line plures/no-raw-stores
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

	// eslint-disable-next-line plures/no-raw-stores
	let kindCounts = $derived(() => {
		const counts: Record<string, number> = { all: 0 };
		for (const schema of Object.values(registry)) {
			counts[schema.kind] = (counts[schema.kind] ?? 0) + 1;
			counts.all++;
		}
		return counts;
	});

	// eslint-disable-next-line plures/no-raw-stores
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

	// eslint-disable-next-line plures/no-raw-stores
	let decisionLedger = $derived(getDecisionLedger());
</script>

<Box class="design-page">
	<Box as="header" class="design-header">
		<Heading level={1}>🎨 Design Mode — Schema Explorer</Heading>
		<Text as="p" class="subtitle">
			{#if designModeActive}
				Editing enabled — select a schema to modify
			{:else}
				Read-only — enter design mode (Ctrl+Shift+D) to edit
			{/if}
		</Text>
	</Box>

	<Box class="design-layout">
		<!-- Filter sidebar -->
		<Box as="aside" class="filter-sidebar">
			<Input
				type="search"
				placeholder="Search schemas..."
				bind:value={searchQuery}
				class="search-input"
			/>

			<Box as="nav" class="kind-filters">
				{#each Object.entries(kindIcons) as [kind, icon]}
					<Button
						variant="ghost"
						class={selectedKind === kind ? 'kind-btn active' : 'kind-btn'}
						onclick={() => selectedKind = kind as SchemaKind | 'all'}
					>
						<Text as="span" class="kind-icon">{icon}</Text>
						<Text as="span" class="kind-label">{kind}</Text>
						<Text as="span" class="kind-count">{kindCounts()[kind] ?? 0}</Text>
					</Button>
				{/each}
			</Box>

			<Box class="schema-list">
				{#each schemas() as schema}
					<Button
						variant="ghost"
						class={selectedSchema === schema.id ? 'schema-item selected' : 'schema-item'}
						onclick={() => selectSchema(schema.id)}
					>
						<Text as="span" class="schema-icon">{kindIcons[schema.kind]}</Text>
						<Box class="schema-info">
							<Text as="span" class="schema-id">{schema.id}</Text>
							<Text as="span" class="schema-module">{schema.moduleId}</Text>
						</Box>
						{#if schema.userCreated}
							<Badge variant="success">user</Badge>
						{/if}
					</Button>
				{/each}
			</Box>
		</Box>

	<!-- Detail panel -->
		<Box as="main" class="detail-panel">
			{#if editingSchema && registry[editingSchema]}
				<RuleEditor
					schema={registry[editingSchema]}
					onSave={handleSave}
					onCancel={cancelEditing}
				/>
			{:else if selectedSchemaData}
				<Box class="schema-detail">
					<Box class="detail-header">
						<Text as="span" class="detail-icon">{kindIcons[selectedSchemaData.kind]}</Text>
						<Box>
							<Heading level={2}>{selectedSchemaData.id}</Heading>
							<Text as="p" class="detail-module">Module: {selectedSchemaData.moduleId}</Text>
						</Box>
						<Box class="detail-kind-badge"><Badge variant="neutral">{selectedSchemaData.kind}</Badge></Box>
					</Box>

					<Text as="p" class="detail-description">{selectedSchemaData.description}</Text>

					<Box as="section" class="definition-section">
						<Heading level={3}>Definition</Heading>
						<CodeBlock class="definition-json" language="json">{JSON.stringify(selectedSchemaData.definition, null, 2)}</CodeBlock>
					</Box>

					{#if designModeActive}
						<Box class="edit-actions">
							<Button variant="primary" class="btn-primary" onclick={() => startEditing(selectedSchemaData?.id ?? '')}>
								✏️ Edit Schema
							</Button>
							{#if selectedSchemaData.userCreated}
								<Button variant="danger" class="btn-danger" onclick={() => {
									emitFact('design.schema.deleted', { schemaId: selectedSchemaData?.id });
								}}>
									🗑️ Delete
								</Button>
							{/if}
						</Box>
					{/if}

					<Box as="footer" class="detail-footer">
						<Text as="span">Last modified: {selectedSchemaData.updatedAt}</Text>
						{#if selectedSchemaData.userCreated}
							<Text as="span">User-created</Text>
						{:else}
							<Text as="span">Built-in</Text>
						{/if}
					</Box>
				</Box>
			{:else}
				<Box class="empty-state">
					<Text as="span" class="empty-icon">🎨</Text>
					<Heading level={2}>Select a schema</Heading>
					<Text as="p">Choose a praxis primitive from the list to view its definition</Text>
				</Box>
			{/if}
		</Box>
	</Box>

	<!-- Decision Ledger -->
	{#if designModeActive}
		<Box as="footer" class="ledger-bar">
			<Button variant="ghost" class="ledger-toggle" onclick={() => showLedger = !showLedger}>
				📜 Decision Ledger ({decisionLedger.length} entries)
				<Text as="span" class="chevron">{showLedger ? '▲' : '▼'}</Text>
			</Button>
			{#if showLedger && decisionLedger.length > 0}
				<Box class="ledger-entries">
					{#each decisionLedger.toReversed() as entry}
						<Box class="ledger-entry">
							<Text as="span" class="ledger-action">{entry.action}</Text>
							<Text as="span" class="ledger-schema">{entry.schemaId}</Text>
							<Text as="span" class="ledger-time">{new Date(entry.timestamp).toLocaleTimeString()}</Text>
							{#if entry.hotReloadResult.applied}
								<Box class="ledger-status ok"><Badge variant="success">✅ applied</Badge></Box>
							{:else}
								<Box class="ledger-status err"><Badge variant="danger">❌ {entry.hotReloadResult.error}</Badge></Box>
							{/if}
						</Box>
					{/each}
				</Box>
			{/if}
		</Box>
	{/if}
</Box>

<style>
	:global(.design-page) {
		padding: 1.5rem;
		height: 100%;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	:global(.design-header) {
		margin-bottom: 1rem;
	}

	:global(.design-header :is(h1, .heading)) {
		font-size: 1.5rem;
		margin: 0;
	}

	:global(.subtitle) {
		color: var(--color-text-muted);
		margin: 0.25rem 0 0;
		font-size: 0.875rem;
	}

	:global(.design-layout) {
		display: flex;
		gap: 1rem;
		flex: 1;
		min-height: 0;
	}

	:global(.filter-sidebar) {
		width: 320px;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		overflow: hidden;
	}

	:global(.search-input) {
		padding: 0.5rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface);
		color: var(--color-text);
		font-size: 0.875rem;
	}

	:global(.kind-filters) {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
	}

	:global(.kind-btn) {
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

	:global(.kind-btn.active) {
		background: var(--color-accent-bg);
		color: var(--color-accent);
		border-color: var(--color-accent);
	}

	:global(.kind-count) {
		background: var(--color-hover);
		padding: 0 0.25rem;
		border-radius: 3px;
		font-size: 0.7rem;
	}

	:global(.schema-list) {
		flex: 1;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	:global(.schema-item) {
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

	:global(.schema-item:hover) { background: var(--color-hover); }
	:global(.schema-item.selected) { background: var(--color-accent-bg); }

	:global(.schema-info) {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	:global(.schema-id) {
		font-size: 0.8rem;
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	:global(.schema-module) {
		font-size: 0.7rem;
		color: var(--color-text-muted);
	}

	:global(.user-badge) {
		font-size: 0.65rem;
		padding: 1px 4px;
		border-radius: 3px;
		background: var(--color-accent-bg);
		color: var(--color-accent);
	}

	:global(.detail-panel) {
		flex: 1;
		overflow-y: auto;
	}

	:global(.schema-detail) {
		padding: 1rem;
	}

	:global(.detail-header) {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}

	:global(.detail-icon) { font-size: 2rem; }

	:global(.detail-header :is(h2, .heading)) {
		margin: 0;
		font-size: 1.25rem;
	}

	:global(.detail-module) {
		margin: 0;
		font-size: 0.8rem;
		color: var(--color-text-muted);
	}

	:global(.detail-kind-badge) {
		margin-left: auto;
		padding: 0.25rem 0.5rem;
		border-radius: 4px;
		background: var(--color-accent-bg);
		color: var(--color-accent);
		font-size: 0.75rem;
		font-weight: 500;
		text-transform: uppercase;
	}

	:global(.detail-description) {
		color: var(--color-text-muted);
		line-height: 1.5;
	}

	:global(.definition-section :is(h3, .heading)) {
		font-size: 0.875rem;
		margin: 1.5rem 0 0.5rem;
		color: var(--color-text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	:global(.definition-json) {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 1rem;
		font-family: 'JetBrains Mono', 'Fira Code', monospace;
		font-size: 0.8rem;
		overflow-x: auto;
		line-height: 1.5;
	}

	:global(.edit-actions) {
		display: flex;
		gap: 0.5rem;
		margin-top: 1.5rem;
	}

	:global(.btn-primary) {
		padding: 0.5rem 1rem;
		border: none;
		border-radius: 6px;
		background: var(--color-accent);
		color: white;
		cursor: pointer;
		font-weight: 500;
	}

	:global(.btn-danger) {
		padding: 0.5rem 1rem;
		border: 1px solid var(--color-danger);
		border-radius: 6px;
		background: transparent;
		color: var(--color-danger);
		cursor: pointer;
		font-weight: 500;
	}

	:global(.detail-footer) {
		display: flex;
		justify-content: space-between;
		margin-top: 2rem;
		padding-top: 1rem;
		border-top: 1px solid var(--color-border);
		font-size: 0.75rem;
		color: var(--color-text-muted);
	}

	:global(.empty-state) {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--color-text-muted);
	}

	:global(.empty-icon) { font-size: 3rem; margin-bottom: 1rem; }
	:global(.empty-state :is(h2, .heading)) { margin: 0; }
	:global(.empty-state p) { margin: 0.5rem 0 0; }

	/* Decision Ledger */
	:global(.ledger-bar) {
		border-top: 1px solid var(--color-border);
		padding: 0.5rem 0;
	}

	:global(.ledger-toggle) {
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

	:global(.ledger-entries) {
		max-height: 200px;
		overflow-y: auto;
		padding: 0.5rem;
	}

	:global(.ledger-entry) {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.25rem 0;
		font-size: 0.75rem;
		border-bottom: 1px solid var(--color-border);
	}

	:global(.ledger-action) {
		padding: 0.1rem 0.35rem;
		border-radius: 3px;
		background: var(--color-accent-bg);
		color: var(--color-accent);
		font-weight: 500;
		text-transform: uppercase;
		font-size: 0.65rem;
	}

	:global(.ledger-schema) {
		font-family: 'JetBrains Mono', monospace;
		flex: 1;
	}

	:global(.ledger-time) { color: var(--color-text-muted); }
	:global(.ledger-status.ok) { color: #22c55e; }
	:global(.ledger-status.err) { color: var(--color-danger); }
</style>
