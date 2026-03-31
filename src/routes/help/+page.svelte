<script lang="ts">
	import { getAllHelpSections } from '$lib/platform/plugin-loader.js';
	import { renderMarkdown } from '$lib/utils/markdown.js';
	import { browser } from '$app/environment';
	import type { HelpSection } from '$lib/types/plugin.js';

	let sections = $derived(getAllHelpSections());
	let searchQuery = $state('');

	let filtered = $derived(
		searchQuery
			? sections.filter((s: HelpSection) =>
					s.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
					(typeof s.content === 'string' && s.content.toLowerCase().includes(searchQuery.toLowerCase()))
				)
			: sections
	);

	// Platform-aware modifier key (⌘ on macOS, Ctrl elsewhere)
	let isMac = $derived(
		browser &&
			/Mac/.test(
				// Use modern userAgentData when available, fall back to userAgent string
				(navigator as Navigator & { userAgentData?: { platform: string } }).userAgentData?.platform ??
					navigator.userAgent
			)
	);
	let mod = $derived(isMac ? '⌘' : 'Ctrl');

	let shortcuts = $derived([
		{ key: `${mod} + K`, desc: 'Quick search' },
		{ key: `${mod} + /`, desc: 'Toggle sidebar' },
		{ key: `${mod} + ,`, desc: 'Settings' },
		{ key: '?', desc: 'This help page' },
	]);
</script>

<svelte:head>
	<title>Radix — Help</title>
</svelte:head>

<h1>Help</h1>

<div class="search-bar">
	<input
		type="search"
		class="search-input"
		placeholder="Search help…"
		aria-label="Search help sections"
		bind:value={searchQuery}
	/>
</div>

{#if filtered.length > 0}
	<div class="sections">
		{#each filtered as section}
			<article class="section" aria-label={section.title}>
				<h2><span class="section-icon" aria-hidden="true">{section.icon}</span> {section.title}</h2>
				{#if typeof section.content === 'string'}
					<div class="section-content markdown">{@html renderMarkdown(section.content)}</div>
				{:else}
					{#await section.content() then mod}
						<div class="section-content">
							<mod.default />
						</div>
					{:catch}
						<p class="error">Failed to load section content.</p>
					{/await}
				{/if}
				{#if section.links?.length}
					<div class="section-links">
						<span class="links-label">See also:</span>
						{#each section.links as link}
							<a href={link.href} class="section-link">{link.label}</a>
						{/each}
					</div>
				{/if}
			</article>
		{/each}
	</div>
{:else}
	<p class="no-results">No help sections match your search.</p>
{/if}

<div class="shortcuts-section" aria-label="Keyboard shortcuts">
	<h2>⌨️ Keyboard Shortcuts</h2>
	<div class="shortcuts-grid">
		{#each shortcuts as shortcut}
			<div class="shortcut">
				<kbd>{shortcut.key}</kbd>
				<span>{shortcut.desc}</span>
			</div>
		{/each}
	</div>
</div>

<style>
	h1 {
		margin: 0 0 16px;
	}

	.search-bar {
		margin-bottom: 24px;
	}

	.search-input {
		width: 100%;
		max-width: 400px;
		padding: 10px 14px;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: var(--color-surface);
		color: var(--color-text);
		font-size: 0.9rem;
	}

	.search-input::placeholder {
		color: var(--color-text-muted);
	}

	.sections {
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	.section {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 20px;
	}

	.section h2 {
		margin: 0 0 12px;
		font-size: 1.05rem;
	}

	.section-icon {
		margin-right: 4px;
	}

	.section-content {
		color: var(--color-text-muted);
		font-size: 0.9rem;
		line-height: 1.6;
	}

	/* Markdown rendered content */
	.section-content.markdown :global(h1),
	.section-content.markdown :global(h2),
	.section-content.markdown :global(h3) {
		color: var(--color-text);
		margin: 12px 0 6px;
		font-size: 0.95rem;
	}

	.section-content.markdown :global(p) {
		margin: 0 0 8px;
	}

	.section-content.markdown :global(p:last-child) {
		margin-bottom: 0;
	}

	.section-content.markdown :global(ul),
	.section-content.markdown :global(ol) {
		margin: 4px 0 8px;
		padding-left: 20px;
	}

	.section-content.markdown :global(li) {
		margin-bottom: 2px;
	}

	.section-content.markdown :global(code) {
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 3px;
		padding: 1px 5px;
		font-size: 0.82rem;
		font-family: ui-monospace, 'SFMono-Regular', monospace;
	}

	.section-content.markdown :global(strong) {
		color: var(--color-text);
		font-weight: 600;
	}

	.section-content.markdown :global(a) {
		color: var(--color-accent);
		text-decoration: none;
	}

	.section-content.markdown :global(a:hover) {
		text-decoration: underline;
	}

	.section-content.markdown :global(hr) {
		border: none;
		border-top: 1px solid var(--color-border);
		margin: 12px 0;
	}

	/* Section "See also" links */
	.section-links {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin-top: 14px;
		padding-top: 12px;
		border-top: 1px solid var(--color-border);
	}

	.links-label {
		font-size: 0.8rem;
		color: var(--color-text-muted);
	}

	.section-link {
		font-size: 0.82rem;
		color: var(--color-accent);
		text-decoration: none;
		padding: 2px 8px;
		border: 1px solid var(--color-accent);
		border-radius: 12px;
		transition: background 0.12s;
	}

	.section-link:hover {
		background: var(--color-accent-bg);
	}

	.no-results {
		color: var(--color-text-muted);
		text-align: center;
		padding: 32px;
	}

	.error {
		color: var(--color-danger);
	}

	.shortcuts-section {
		margin-top: 32px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 20px;
	}

	.shortcuts-section h2 {
		margin: 0 0 16px;
		font-size: 1.05rem;
	}

	.shortcuts-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
		gap: 12px;
	}

	.shortcut {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	kbd {
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 4px;
		padding: 2px 8px;
		font-size: 0.8rem;
		font-family: monospace;
		white-space: nowrap;
	}

	.shortcut span {
		font-size: 0.85rem;
		color: var(--color-text-muted);
	}
</style>

