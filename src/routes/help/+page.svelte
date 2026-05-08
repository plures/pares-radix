<script lang="ts">
	import { getAllHelpSections } from '$lib/platform/plugin-loader.js';
	import { renderMarkdown, isSafeUrl } from '$lib/utils/markdown.js';
	import { browser } from '$app/environment';
	import type { HelpSection } from '$lib/types/plugin.js';
	import { Box, Heading, Input, Link, Text } from '@plures/design-dojo';

	// eslint-disable-next-line plures/no-raw-stores
	let sections = $derived(getAllHelpSections());
	// eslint-disable-next-line plures/no-raw-stores
	let searchQuery = $state('');

	// eslint-disable-next-line plures/no-raw-stores
	let filtered = $derived(
		searchQuery
			? sections.filter((s: HelpSection) =>
					s.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
					(typeof s.content === 'string' && s.content.toLowerCase().includes(searchQuery.toLowerCase()))
				)
			: sections
	);

	// Platform-aware modifier key (⌘ on macOS, Ctrl elsewhere)
	// eslint-disable-next-line plures/no-raw-stores
	let isMac = $derived(
		browser &&
			/Mac/.test(
				// Use modern userAgentData when available, fall back to userAgent string
				(navigator as Navigator & { userAgentData?: { platform: string } }).userAgentData?.platform ??
					navigator.userAgent
			)
	);
	// eslint-disable-next-line plures/no-raw-stores
	let mod = $derived(isMac ? '⌘' : 'Ctrl');

	// eslint-disable-next-line plures/no-raw-stores
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

<Heading level={1}>Help</Heading>

<Box class="search-bar">
	<Input
		type="search"
		class="search-input"
		placeholder="Search help…"
		aria-label="Search help sections"
		bind:value={searchQuery}
	/>
</Box>

{#if filtered.length > 0}
	<Box class="sections">
		{#each filtered as section}
			<Box as="article" class="section" aria-label={section.title}>
				<Heading level={2}>
					<Text as="span" class="section-icon" aria-hidden="true">{section.icon}</Text>
					{section.title}
				</Heading>
				{#if typeof section.content === 'string'}
					<Box class="section-content markdown">{@html renderMarkdown(section.content)}</Box>
				{:else}
					{#await section.content() then mod}
						<Box class="section-content">
							<mod.default />
						</Box>
					{:catch}
						<Text as="p" class="error">Failed to load section content.</Text>
					{/await}
				{/if}
				{#if section.links?.length}
					<Box class="section-links">
						<Text as="span" class="links-label">See also:</Text>
						{#each section.links as link}
							{#if isSafeUrl(link.href)}
								<Link href={link.href} class="section-link">{link.label}</Link>
							{/if}
						{/each}
					</Box>
				{/if}
			</Box>
		{/each}
	</Box>
{:else}
	<Text as="p" class="no-results">No help sections match your search.</Text>
{/if}

<Box class="shortcuts-section" aria-label="Keyboard shortcuts">
	<Heading level={2}>⌨️ Keyboard Shortcuts</Heading>
	<Box class="shortcuts-grid">
		{#each shortcuts as shortcut}
			<Box class="shortcut" direction="row" align="center" gap="12px">
				<Text as="span" class="kbd">{shortcut.key}</Text>
				<Text as="span">{shortcut.desc}</Text>
			</Box>
		{/each}
	</Box>
</Box>

<style>
	:global(h1) {
		margin: 0 0 16px;
	}

	:global(.search-bar) {
		margin-bottom: 24px;
	}

	:global(.search-input) {
		width: 100%;
		max-width: 400px;
	}

	:global(.sections) {
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	:global(.section) {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 20px;
	}

	:global(.section h2) {
		margin: 0 0 12px;
		font-size: 1.05rem;
	}

	:global(.section-icon) {
		margin-right: 4px;
	}

	:global(.section-content) {
		color: var(--color-text-muted);
		font-size: 0.9rem;
		line-height: 1.6;
	}

	/* Markdown rendered content */
	:global(.section-content.markdown :global(h1)),
	:global(.section-content.markdown :global(h2)),
	:global(.section-content.markdown :global(h3)) {
		color: var(--color-text);
		margin: 12px 0 6px;
		font-size: 0.95rem;
	}

	:global(.section-content.markdown :global(p)) {
		margin: 0 0 8px;
	}

	:global(.section-content.markdown :global(p:last-child)) {
		margin-bottom: 0;
	}

	:global(.section-content.markdown :global(ul)),
	:global(.section-content.markdown :global(ol)) {
		margin: 4px 0 8px;
		padding-left: 20px;
	}

	:global(.section-content.markdown :global(li)) {
		margin-bottom: 2px;
	}

	:global(.section-content.markdown :global(code)) {
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 3px;
		padding: 1px 5px;
		font-size: 0.82rem;
		font-family: ui-monospace, 'SFMono-Regular', monospace;
	}

	:global(.section-content.markdown :global(strong)) {
		color: var(--color-text);
		font-weight: 600;
	}

	:global(.section-content.markdown :global(a)) {
		color: var(--color-accent);
		text-decoration: none;
	}

	:global(.section-content.markdown :global(a:hover)) {
		text-decoration: underline;
	}

	:global(.section-content.markdown :global(hr)) {
		border: none;
		border-top: 1px solid var(--color-border);
		margin: 12px 0;
	}

	/* Section "See also" links */
	:global(.section-links) {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin-top: 14px;
		padding-top: 12px;
		border-top: 1px solid var(--color-border);
	}

	:global(.links-label) {
		font-size: 0.8rem;
		color: var(--color-text-muted);
	}

	:global(.section-link) {
		font-size: 0.82rem;
		color: var(--color-accent);
		text-decoration: none;
		padding: 2px 8px;
		border: 1px solid var(--color-accent);
		border-radius: 12px;
		transition: background 0.12s;
	}

	:global(.section-link:hover) {
		background: var(--color-accent-bg);
	}

	:global(.no-results) {
		color: var(--color-text-muted);
		text-align: center;
		padding: 32px;
	}

	:global(.error) {
		color: var(--color-danger);
	}

	:global(.shortcuts-section) {
		margin-top: 32px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 20px;
	}

	:global(.shortcuts-section h2) {
		margin: 0 0 16px;
		font-size: 1.05rem;
	}

	:global(.shortcuts-grid) {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
		gap: 12px;
	}

	:global(.shortcut) {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	:global(.kbd) {
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 4px;
		padding: 2px 8px;
		font-size: 0.8rem;
		font-family: monospace;
		white-space: nowrap;
	}

	:global(.shortcut span) {
		font-size: 0.85rem;
		color: var(--color-text-muted);
	}
</style>

