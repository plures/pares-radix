<script lang="ts">
	/**
	 * LIVE interactive showcase for the Wt pane primitives.
	 * Everything here is real: the sash drags, tabs close/reorder, the pane collapses.
	 */
	import {
		WtSplitPane,
		WtPane,
		WtPaneTabs,
		Box,
		Heading,
		Text,
		Button,
		type WtTabDescriptor
	} from '@plures/design-dojo';

	// eslint-disable-next-line plures/no-raw-stores -- local UI-only view state (pane sizes/tabs), not persisted domain data
	let splitSize = $state(280);
	// eslint-disable-next-line plures/no-raw-stores -- local UI-only view state
	let splitCollapsed = $state(false);
	// eslint-disable-next-line plures/no-raw-stores -- local UI-only view state
	let paneCollapsed = $state(false);

	// eslint-disable-next-line plures/no-raw-stores -- local UI-only view state
	let tabs = $state<WtTabDescriptor[]>([
		{ id: 'terminal', title: 'Terminal', icon: '❯', closable: true },
		{ id: 'problems', title: 'Problems', icon: '⚠', closable: true },
		{ id: 'output', title: 'Output', icon: '≡', closable: true }
	]);
	// eslint-disable-next-line plures/no-raw-stores -- local UI-only view state
	let activeTab = $state<string | null>('terminal');

	const bodies: Record<string, string> = {
		terminal: 'A live terminal view. Reorder these tabs by dragging, or close one with ×.',
		problems: 'No problems detected. This panel switches in real time when you select its tab.',
		output: 'Build output would stream here. Try keyboard nav: focus a tab, use ← → Home End.'
	};
</script>

<svelte:head>
	<title>Panes — Design Dojo</title>
</svelte:head>

<Box class="panes-showcase">
	<Heading level={1}>Pane Primitives</Heading>
	<Text as="p" class="lede">
		Interactive VS Code–style pane primitives. Drag the sash, collapse the pane,
		reorder and close tabs — all real, no fixtures.
	</Text>

	<Box as="section">
		<Heading level={2}>WtSplitPane — draggable sash</Heading>
		<Text as="p" class="hint">
			size = {Math.round(splitSize)}px · collapsed = {splitCollapsed}
			· drag the divider, or focus it and use arrow keys / Home / End. Double-click the sash to collapse.
		</Text>
		<Box class="stage">
			<WtSplitPane
				bind:size={splitSize}
				collapsed={splitCollapsed}
				minSize={120}
				minSecondary={120}
				orientation="horizontal"
				onresize={(s) => (splitSize = s)}
				oncollapse={(c) => (splitCollapsed = c)}
			>
				{#snippet a()}
					<Box class="fill pane-a"><Text as="span">Primary pane A</Text></Box>
				{/snippet}
				{#snippet b()}
					<Box class="fill pane-b"><Text as="span">Secondary pane B</Text></Box>
				{/snippet}
			</WtSplitPane>
		</Box>
	</Box>

	<Box as="section">
		<Heading level={2}>WtPane — collapsible dock</Heading>
		<Box class="stage stage-short">
			<WtPane
				title="Agens"
				icon="🤖"
				bind:collapsed={paneCollapsed}
				oncollapse={(c) => (paneCollapsed = c)}
			>
				{#snippet actions()}
					<Button variant="ghost" class="mini-action" aria-label="Refresh">⟳</Button>
				{/snippet}
				<Text as="p">
					A titled, collapsible dock region. Click the chevron in the header to
					collapse/expand — the body actually hides. collapsed = {paneCollapsed}.
				</Text>
			</WtPane>
		</Box>
	</Box>

	<Box as="section">
		<Heading level={2}>WtPaneTabs — closable, reorderable tabs</Heading>
		<Text as="p" class="hint">active = {activeTab ?? '(none)'} · {tabs.length} tab(s)</Text>
		<Box class="stage stage-short">
			<WtPaneTabs bind:tabs bind:active={activeTab}>
				{#snippet panel(id)}
					<Text as="p">{bodies[id] ?? `View: ${id}`}</Text>
				{/snippet}
			</WtPaneTabs>
		</Box>
	</Box>
</Box>

<style>
	:global(.panes-showcase) {
		padding: 16px;
		color: var(--color-text);
		display: flex;
		flex-direction: column;
		gap: 20px;
	}
	:global(.panes-showcase > section) {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	:global(.lede) {
		margin: 0;
		color: var(--color-text-muted, var(--color-text));
		max-width: 60ch;
	}
	:global(.hint) {
		margin: 0;
		font-size: 0.8rem;
		color: var(--color-text-muted, var(--color-text));
	}
	:global(.stage) {
		height: 240px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		overflow: hidden;
		background: var(--color-surface);
	}
	:global(.stage-short) {
		height: 180px;
	}
	:global(.fill) {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		font-weight: 600;
	}
	:global(.pane-a) {
		background: var(--color-surface-alt, var(--color-surface));
		color: var(--color-text);
	}
	:global(.pane-b) {
		background: var(--color-accent, var(--color-primary, #7c8cff));
		color: var(--color-on-accent, #ffffff);
	}
	:global(.mini-action) {
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-text);
		border-radius: 3px;
		cursor: pointer;
		width: 20px;
		height: 20px;
		line-height: 1;
	}
	:global(.mini-action:hover) {
		background: var(--color-border);
	}
</style>
