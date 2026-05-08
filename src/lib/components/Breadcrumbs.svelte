<script lang="ts">
	import { page } from '$app/state';
	import { Box, List, ListItem, Link, Text } from '@plures/design-dojo';

	interface Crumb {
		label: string;
		href?: string;
	}

	interface Props {
		/** Override crumbs; if not provided, auto-generated from URL path */
		crumbs?: Crumb[];
	}

	let { crumbs }: Props = $props();

	// Auto-generate breadcrumbs from the current path if none provided
	let autoCrumbs = $derived.by(() => {
		if (crumbs && crumbs.length > 0) return crumbs;
		const segments = page.url.pathname.split('/').filter(Boolean);
		const result: Crumb[] = [{ label: 'Home', href: '/' }];
		let path = '';
		for (const seg of segments) {
			path += `/${seg}`;
			result.push({
				label: seg.charAt(0).toUpperCase() + seg.slice(1).replace(/-/g, ' '),
				href: path,
			});
		}
		return result;
	});
</script>

<Box as="nav" class="breadcrumbs" aria-label="Breadcrumb">
	<List ordered>
		{#each autoCrumbs as crumb, i}
			<ListItem>
				{#if i < autoCrumbs.length - 1 && crumb.href}
					<Link href={crumb.href}>{crumb.label}</Link>
					<Text as="span" class="separator" aria-hidden="true">/</Text>
				{:else}
					<Text as="span" class="current" aria-current="page">{crumb.label}</Text>
				{/if}
			</ListItem>
		{/each}
	</List>
</Box>

<style>
	:global(.breadcrumbs) {
		padding: 0.5rem 0;
		font-size: 0.82rem;
	}

	:global(ol) {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		align-items: center;
		gap: 0;
	}

	:global(li) {
		display: flex;
		align-items: center;
	}

	:global(a) {
		color: var(--color-text-muted);
		text-decoration: none;
		padding: 2px 4px;
		border-radius: 3px;
		transition: color 0.15s, background 0.15s;
	}

	:global(a:hover) {
		color: var(--color-accent);
		background: var(--color-accent-bg);
	}

	:global(.separator) {
		margin: 0 6px;
		color: var(--color-border);
	}

	:global(.current) {
		color: var(--color-text);
		font-weight: 500;
	}
</style>
