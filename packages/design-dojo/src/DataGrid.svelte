<script lang="ts">
	import type { DataGridProps, DataRow, SchemaField, SortDirection } from './types-local.js';

	let {
		schema,
		rows,
		pageSize = 25,
		filterable = true,
		sortable = true,
		onRowClick,
		class: className = ''
	}: DataGridProps = $props();

	// Visible columns = non-hidden schema fields.
	const columns = $derived(schema.fields.filter((f) => !f.hidden));

	function columnLabel(field: SchemaField): string {
		if (field.label) return field.label;
		return field.name
			.replace(/[_-]+/g, ' ')
			.replace(/([a-z])([A-Z])/g, '$1 $2')
			.replace(/^\w/, (c) => c.toUpperCase());
	}

	// --- Filtering -----------------------------------------------------------
	let filters = $state<Record<string, string>>({});

	function displayValue(value: unknown): string {
		if (value === null || value === undefined) return '';
		if (value instanceof Date) return value.toISOString();
		if (typeof value === 'boolean') return value ? 'true' : 'false';
		return String(value);
	}

	const filteredRows = $derived.by(() => {
		const active = Object.entries(filters).filter(([, v]) => v.trim() !== '');
		if (active.length === 0) return rows;
		return rows.filter((row) =>
			active.every(([name, needle]) =>
				displayValue(row[name]).toLowerCase().includes(needle.toLowerCase())
			)
		);
	});

	// --- Sorting -------------------------------------------------------------
	let sortField = $state<string | null>(null);
	let sortDir = $state<SortDirection>('asc');

	function compare(a: unknown, b: unknown, type: SchemaField['type']): number {
		if (a === null || a === undefined) return b === null || b === undefined ? 0 : -1;
		if (b === null || b === undefined) return 1;
		if (type === 'number') return Number(a) - Number(b);
		if (type === 'boolean') return (a ? 1 : 0) - (b ? 1 : 0);
		if (type === 'datetime') return new Date(a as string).getTime() - new Date(b as string).getTime();
		return String(a).localeCompare(String(b));
	}

	const sortedRows = $derived.by(() => {
		if (!sortField) return filteredRows;
		const field = columns.find((c) => c.name === sortField);
		if (!field) return filteredRows;
		const dir = sortDir === 'asc' ? 1 : -1;
		return [...filteredRows].sort((r1, r2) => dir * compare(r1[field.name], r2[field.name], field.type));
	});

	function toggleSort(field: SchemaField) {
		if (!sortable) return;
		if (sortField === field.name) {
			sortDir = sortDir === 'asc' ? 'desc' : 'asc';
		} else {
			sortField = field.name;
			sortDir = 'asc';
		}
		page = 0;
	}

	// --- Pagination ----------------------------------------------------------
	let page = $state(0);
	const paginated = $derived(pageSize > 0);
	const pageCount = $derived(paginated ? Math.max(1, Math.ceil(sortedRows.length / pageSize)) : 1);
	// Clamp page if the data shrank underneath us.
	$effect(() => {
		if (page > pageCount - 1) page = pageCount - 1;
		if (page < 0) page = 0;
	});
	const visibleRows = $derived(
		paginated ? sortedRows.slice(page * pageSize, page * pageSize + pageSize) : sortedRows
	);

	function rowKey(row: DataRow, index: number): string {
		const id = row.id ?? row._id ?? row.key;
		return id !== undefined ? String(id) : `row-${index}`;
	}
</script>

<div class="datagrid {className}">
	<div class="table-scroll" role="region" aria-label={schema.name ?? 'Data grid'}>
		<table class="table">
			<thead>
				<tr>
					{#each columns as col (col.name)}
						<th
							class:sortable
							aria-sort={sortField === col.name
								? sortDir === 'asc'
									? 'ascending'
									: 'descending'
								: 'none'}
						>
							<button
								type="button"
								class="th-btn"
								disabled={!sortable}
								onclick={() => toggleSort(col)}
								title={col.description}
							>
								<span class="th-label">{columnLabel(col)}</span>
								{#if sortable && sortField === col.name}
									<span class="sort-ind" aria-hidden="true">{sortDir === 'asc' ? '▲' : '▼'}</span>
								{/if}
							</button>
						</th>
					{/each}
				</tr>
				{#if filterable}
					<tr class="filter-row">
						{#each columns as col (col.name)}
							<th>
								<input
									class="filter-input"
									type="text"
									placeholder="Filter…"
									aria-label={`Filter by ${columnLabel(col)}`}
									bind:value={
										() => filters[col.name] ?? '',
										(v) => {
											filters[col.name] = v;
											page = 0;
										}
									}
								/>
							</th>
						{/each}
					</tr>
				{/if}
			</thead>
			<tbody>
				{#each visibleRows as row, i (rowKey(row, i))}
					<tr
						class:clickable={!!onRowClick}
						onclick={() => onRowClick?.(row)}
					>
						{#each columns as col (col.name)}
							<td>
								{#if col.type === 'boolean'}
									<span class="bool" class:on={!!row[col.name]}>{row[col.name] ? '✓' : '✗'}</span>
								{:else}
									{displayValue(row[col.name])}
								{/if}
							</td>
						{/each}
					</tr>
				{/each}

				{#if visibleRows.length === 0}
					<tr>
						<td class="empty" colspan={columns.length}>No rows to display.</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>

	{#if paginated && pageCount > 1}
		<div class="pager">
			<button type="button" class="pager-btn" disabled={page === 0} onclick={() => (page = 0)} aria-label="First page">«</button>
			<button type="button" class="pager-btn" disabled={page === 0} onclick={() => (page -= 1)} aria-label="Previous page">‹</button>
			<span class="pager-status">Page {page + 1} of {pageCount} · {sortedRows.length} rows</span>
			<button type="button" class="pager-btn" disabled={page >= pageCount - 1} onclick={() => (page += 1)} aria-label="Next page">›</button>
			<button type="button" class="pager-btn" disabled={page >= pageCount - 1} onclick={() => (page = pageCount - 1)} aria-label="Last page">»</button>
		</div>
	{/if}
</div>

<style>
	.datagrid {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.table-scroll {
		overflow-x: auto;
		border: 1px solid var(--color-border);
		border-radius: 8px;
	}

	.table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.85rem;
		color: var(--color-text);
	}

	.table thead {
		background: var(--color-surface);
	}

	.table th {
		text-align: left;
		border-bottom: 1px solid var(--color-border);
		padding: 0;
		white-space: nowrap;
	}

	.th-btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		background: transparent;
		border: none;
		color: var(--color-text-muted);
		font: inherit;
		font-weight: 600;
		padding: 9px 12px;
		text-align: left;
		cursor: default;
	}

	.th-btn:not(:disabled) {
		cursor: pointer;
	}

	th.sortable .th-btn:hover {
		background: var(--color-hover);
		color: var(--color-text);
	}

	.sort-ind {
		font-size: 0.7rem;
		color: var(--color-accent, #6366f1);
	}

	.filter-row th {
		padding: 6px 8px;
		background: var(--color-surface);
	}

	.filter-input {
		width: 100%;
		box-sizing: border-box;
		padding: 4px 8px;
		border-radius: 5px;
		border: 1px solid var(--color-border);
		background: var(--color-bg, var(--color-surface));
		color: var(--color-text);
		font-size: 0.8rem;
		outline: none;
	}

	.filter-input:focus {
		border-color: var(--color-accent, #6366f1);
	}

	.table td {
		padding: 8px 12px;
		border-bottom: 1px solid var(--color-border);
		white-space: nowrap;
	}

	.table tbody tr:last-child td {
		border-bottom: none;
	}

	tr.clickable {
		cursor: pointer;
	}

	tr.clickable:hover td {
		background: var(--color-hover);
	}

	.bool {
		color: var(--color-danger, #ef4444);
		font-weight: 700;
	}

	.bool.on {
		color: var(--color-success, #22c55e);
	}

	.empty {
		text-align: center;
		color: var(--color-text-muted);
		padding: 24px;
		white-space: normal;
	}

	.pager {
		display: flex;
		align-items: center;
		gap: 6px;
		justify-content: flex-end;
	}

	.pager-btn {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		color: var(--color-text);
		border-radius: 6px;
		padding: 4px 10px;
		cursor: pointer;
		font-size: 0.85rem;
		transition: background 0.12s;
	}

	.pager-btn:hover:not(:disabled) {
		background: var(--color-hover);
	}

	.pager-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.pager-status {
		font-size: 0.8rem;
		color: var(--color-text-muted);
		margin: 0 4px;
	}
</style>
