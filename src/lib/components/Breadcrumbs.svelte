<script lang="ts">
	import { page } from '$app/state';

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

<nav class="breadcrumbs" aria-label="Breadcrumb">
	<ol>
		{#each autoCrumbs as crumb, i}
			<li>
				{#if i < autoCrumbs.length - 1 && crumb.href}
					<a href={crumb.href}>{crumb.label}</a>
					<span class="separator" aria-hidden="true">/</span>
				{:else}
					<span class="current" aria-current="page">{crumb.label}</span>
				{/if}
			</li>
		{/each}
	</ol>
</nav>

<style>
	.breadcrumbs {
		padding: 0.5rem 0;
		font-size: 0.82rem;
	}

	ol {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		align-items: center;
		gap: 0;
	}

	li {
		display: flex;
		align-items: center;
	}

	a {
		color: var(--color-text-muted);
		text-decoration: none;
		padding: 2px 4px;
		border-radius: 3px;
		transition: color 0.15s, background 0.15s;
	}

	a:hover {
		color: var(--color-accent);
		background: var(--color-accent-bg);
	}

	.separator {
		margin: 0 6px;
		color: var(--color-border);
	}

	.current {
		color: var(--color-text);
		font-weight: 500;
	}
</style>
