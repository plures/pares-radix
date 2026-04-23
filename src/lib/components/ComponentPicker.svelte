<script lang="ts">
	/**
	 * ComponentPicker — browse and select design-dojo components
	 * for schema-driven UI composition in design mode.
	 */

	interface ComponentEntry {
		name: string;
		category: string;
		description: string;
		props: { name: string; type: string; required: boolean }[];
		tuiWidget?: string;
	}

	interface Props {
		onSelect: (component: ComponentEntry) => void;
	}

	let { onSelect }: Props = $props();

	let searchQuery = $state('');
	let selectedCategory = $state<string>('all');

	// design-dojo component catalog — derived from the library's exports
	const catalog: ComponentEntry[] = [
		// Primitives
		{ name: 'Button', category: 'Primitives', description: 'Clickable action trigger', props: [
			{ name: 'variant', type: "'solid' | 'outline' | 'ghost'", required: false },
			{ name: 'onclick', type: '() => void', required: false },
			{ name: 'disabled', type: 'boolean', required: false },
		], tuiWidget: 'Button' },
		{ name: 'Input', category: 'Primitives', description: 'Text input field', props: [
			{ name: 'value', type: 'string', required: false },
			{ name: 'placeholder', type: 'string', required: false },
			{ name: 'type', type: 'string', required: false },
		], tuiWidget: 'Input' },
		{ name: 'Toggle', category: 'Primitives', description: 'Boolean toggle switch', props: [
			{ name: 'checked', type: 'boolean', required: false },
			{ name: 'onchange', type: '(checked: boolean) => void', required: false },
		], tuiWidget: 'Checkbox' },
		{ name: 'Select', category: 'Primitives', description: 'Dropdown selection', props: [
			{ name: 'options', type: '{ value: string; label: string }[]', required: true },
			{ name: 'value', type: 'string', required: false },
		], tuiWidget: 'List' },
		{ name: 'SearchInput', category: 'Primitives', description: 'Search with autocomplete', props: [
			{ name: 'value', type: 'string', required: false },
			{ name: 'results', type: 'SearchResult[]', required: false },
		], tuiWidget: 'Input' },
		{ name: 'Text', category: 'Primitives', description: 'Styled text display', props: [
			{ name: 'variant', type: "'body' | 'heading' | 'caption' | 'mono'", required: false },
		], tuiWidget: 'Span' },
		{ name: 'MarkdownEditor', category: 'Primitives', description: 'Markdown editor with preview', props: [
			{ name: 'value', type: 'string', required: false },
			{ name: 'mode', type: "'edit' | 'preview' | 'split'", required: false },
		], tuiWidget: 'Paragraph' },

		// Layout
		{ name: 'Sidebar', category: 'Layout', description: 'Navigation sidebar panel', props: [
			{ name: 'items', type: 'NavItem[]', required: true },
			{ name: 'currentPath', type: 'string', required: false },
			{ name: 'collapsed', type: 'boolean', required: false },
		], tuiWidget: 'List' },
		{ name: 'StatusBar', category: 'Layout', description: 'Bottom status bar', props: [], tuiWidget: 'Paragraph' },
		{ name: 'TitleBar', category: 'Layout', description: 'Window title bar', props: [
			{ name: 'title', type: 'string', required: false },
		], tuiWidget: 'Block' },
		{ name: 'ActivityBar', category: 'Layout', description: 'Vertical icon strip', props: [
			{ name: 'items', type: 'ActivityItem[]', required: true },
		], tuiWidget: 'Tabs' },
		{ name: 'Tabs', category: 'Layout', description: 'Tab navigation', props: [
			{ name: 'items', type: 'string[]', required: true },
			{ name: 'activeIndex', type: 'number', required: false },
		], tuiWidget: 'Tabs' },
		{ name: 'SplitPane', category: 'Layout', description: 'Resizable split view', props: [
			{ name: 'direction', type: "'horizontal' | 'vertical'", required: false },
		], tuiWidget: 'Layout' },
		{ name: 'DashboardGrid', category: 'Layout', description: 'Responsive grid layout', props: [
			{ name: 'columns', type: 'number', required: false },
		], tuiWidget: 'Layout' },
		{ name: 'Box', category: 'Layout', description: 'Flexbox container', props: [
			{ name: 'direction', type: "'row' | 'column'", required: false },
			{ name: 'gap', type: 'string', required: false },
		], tuiWidget: 'Block' },

		// Data
		{ name: 'Table', category: 'Data', description: 'Data table with sorting', props: [
			{ name: 'columns', type: 'Column[]', required: true },
			{ name: 'rows', type: 'unknown[]', required: true },
		], tuiWidget: 'Table' },
		{ name: 'List', category: 'Data', description: 'Scrollable list', props: [
			{ name: 'items', type: 'unknown[]', required: true },
		], tuiWidget: 'List' },
		{ name: 'TreeView', category: 'Data', description: 'Hierarchical tree', props: [
			{ name: 'nodes', type: 'TreeNode[]', required: true },
		], tuiWidget: 'Tree' },

		// Overlays
		{ name: 'CommandPalette', category: 'Overlays', description: 'Fuzzy command search', props: [
			{ name: 'commands', type: 'CommandItem[]', required: true },
			{ name: 'open', type: 'boolean', required: false },
		], tuiWidget: 'Popup' },
		{ name: 'Dialog', category: 'Overlays', description: 'Modal dialog', props: [
			{ name: 'open', type: 'boolean', required: false },
			{ name: 'title', type: 'string', required: false },
		], tuiWidget: 'Popup' },
		{ name: 'Toast', category: 'Overlays', description: 'Notification toast', props: [
			{ name: 'message', type: 'string', required: true },
			{ name: 'variant', type: "'info' | 'success' | 'warning' | 'error'", required: false },
		], tuiWidget: 'Paragraph' },

		// Surfaces
		{ name: 'Card', category: 'Surfaces', description: 'Content card with border', props: [
			{ name: 'title', type: 'string', required: false },
		], tuiWidget: 'Block' },
		{ name: 'ChatPane', category: 'Surfaces', description: 'Chat message display', props: [
			{ name: 'messages', type: 'Message[]', required: true },
		], tuiWidget: 'List' },

		// Feedback
		{ name: 'ProgressBar', category: 'Feedback', description: 'Progress indicator', props: [
			{ name: 'value', type: 'number', required: true },
			{ name: 'max', type: 'number', required: false },
		], tuiWidget: 'Gauge' },
		{ name: 'Badge', category: 'Feedback', description: 'Status badge', props: [
			{ name: 'variant', type: "'info' | 'success' | 'warning' | 'error'", required: false },
		], tuiWidget: 'Span' },
		{ name: 'EmptyState', category: 'Feedback', description: 'Empty content placeholder', props: [
			{ name: 'icon', type: 'string', required: false },
			{ name: 'message', type: 'string', required: true },
		], tuiWidget: 'Paragraph' },
	];

	const categories = ['all', ...new Set(catalog.map(c => c.category))];

	let filtered = $derived(() => {
		let items = catalog;
		if (selectedCategory !== 'all') {
			items = items.filter(c => c.category === selectedCategory);
		}
		if (searchQuery) {
			const q = searchQuery.toLowerCase();
			items = items.filter(c =>
				c.name.toLowerCase().includes(q) ||
				c.description.toLowerCase().includes(q)
			);
		}
		return items;
	});
</script>

<div class="picker">
	<header class="picker-header">
		<h3>🧩 Component Picker</h3>
		<input
			type="search"
			placeholder="Search components..."
			bind:value={searchQuery}
			class="picker-search"
		/>
	</header>

	<div class="picker-categories">
		{#each categories as cat}
			<button
				class="cat-btn"
				class:active={selectedCategory === cat}
				onclick={() => selectedCategory = cat}
			>
				{cat}
			</button>
		{/each}
	</div>

	<div class="picker-grid">
		{#each filtered() as comp}
			<button class="comp-card" onclick={() => onSelect(comp)}>
				<div class="comp-name">{comp.name}</div>
				<div class="comp-desc">{comp.description}</div>
				<div class="comp-meta">
					<span class="comp-cat">{comp.category}</span>
					{#if comp.tuiWidget}
						<span class="comp-tui">TUI: {comp.tuiWidget}</span>
					{/if}
					<span class="comp-props">{comp.props.length} props</span>
				</div>
			</button>
		{/each}
	</div>
</div>

<style>
	.picker { display: flex; flex-direction: column; gap: 0.75rem; }

	.picker-header {
		display: flex; align-items: center; gap: 1rem;
	}
	.picker-header h3 { margin: 0; font-size: 1rem; }

	.picker-search {
		flex: 1; padding: 0.4rem 0.75rem;
		border: 1px solid var(--color-border); border-radius: 6px;
		background: var(--color-surface); color: var(--color-text);
		font-size: 0.85rem;
	}

	.picker-categories {
		display: flex; flex-wrap: wrap; gap: 0.25rem;
	}

	.cat-btn {
		padding: 0.2rem 0.5rem; border: 1px solid var(--color-border);
		border-radius: 4px; background: var(--color-surface);
		color: var(--color-text-muted); cursor: pointer; font-size: 0.75rem;
	}
	.cat-btn.active {
		background: var(--color-accent-bg); color: var(--color-accent);
		border-color: var(--color-accent);
	}

	.picker-grid {
		display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
		gap: 0.5rem;
	}

	.comp-card {
		padding: 0.75rem; border: 1px solid var(--color-border);
		border-radius: 6px; background: var(--color-surface);
		cursor: pointer; text-align: left; width: 100%;
	}
	.comp-card:hover { border-color: var(--color-accent); }

	.comp-name { font-weight: 600; font-size: 0.9rem; margin-bottom: 0.25rem; }
	.comp-desc { font-size: 0.75rem; color: var(--color-text-muted); margin-bottom: 0.5rem; }

	.comp-meta { display: flex; gap: 0.5rem; flex-wrap: wrap; }
	.comp-cat, .comp-tui, .comp-props {
		font-size: 0.65rem; padding: 0.1rem 0.35rem;
		border-radius: 3px; background: var(--color-hover);
	}
	.comp-tui { background: var(--color-accent-bg); color: var(--color-accent); }
</style>
