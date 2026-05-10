<script lang="ts">
	import { Heading, Input, Select, Button, TextArea, Card, Box } from '@plures/design-dojo';
	import { browser } from '$app/environment';

	interface EquipmentItem {
		id: string;
		name: string;
		category: string;
		location: string;
		serialModel: string;
		condition: string;
		notes: string;
		purchaseDate: string;
		value: string;
	}

	const STORAGE_KEY = 'radix-inventory-items';

	const categories = [
		{ value: 'Tools', label: 'Tools' },
		{ value: 'Vehicles', label: 'Vehicles' },
		{ value: 'Electronics', label: 'Electronics' },
		{ value: 'Furniture', label: 'Furniture' },
		{ value: 'Machinery', label: 'Machinery' },
		{ value: 'Other', label: 'Other' }
	];

	const conditions = [
		{ value: 'Excellent', label: 'Excellent' },
		{ value: 'Good', label: 'Good' },
		{ value: 'Fair', label: 'Fair' },
		{ value: 'Poor', label: 'Poor' },
		{ value: 'Needs Repair', label: 'Needs Repair' }
	];

	// Form state
	let name = $state('');
	let category = $state('');
	let location = $state('');
	let serialModel = $state('');
	let condition = $state('');
	let notes = $state('');
	let purchaseDate = $state('');
	let value = $state('');

	// Items list
	let items = $state<EquipmentItem[]>([]);

	// Search/filter state
	let searchTerm = $state('');
	let filterCategory = $state('');

	// Load items from localStorage on mount
	$effect(() => {
		if (browser) {
			const stored = localStorage.getItem(STORAGE_KEY);
			if (stored) {
				try {
					items = JSON.parse(stored);
				} catch {
					items = [];
				}
			}
		}
	});

	// Save to localStorage whenever items change
	function saveItems() {
		if (browser) {
			localStorage.setItem(STORAGE_KEY, JSON.stringify(items));
		}
	}

	function addItem() {
		if (!name.trim()) {
			alert('Equipment name is required');
			return;
		}

		const newItem: EquipmentItem = {
			id: Date.now().toString(),
			name,
			category,
			location,
			serialModel,
			condition,
			notes,
			purchaseDate,
			value
		};

		items = [...items, newItem];
		saveItems();

		// Clear form
		name = '';
		category = '';
		location = '';
		serialModel = '';
		condition = '';
		notes = '';
		purchaseDate = '';
		value = '';
	}

	function deleteItem(id: string) {
		if (confirm('Delete this item?')) {
			items = items.filter(i => i.id !== id);
			saveItems();
		}
	}

	function exportToCSV() {
		const headers = ['Name', 'Category', 'Location', 'Serial/Model', 'Condition', 'Purchase Date', 'Value', 'Notes'];
		const rows = items.map(item => [
			item.name,
			item.category,
			item.location,
			item.serialModel,
			item.condition,
			item.purchaseDate,
			item.value,
			item.notes
		]);

		const csv = [
			headers.join(','),
			...rows.map(row => row.map(cell => `"${cell.replace(/"/g, '""')}"`).join(','))
		].join('\n');

		const blob = new Blob([csv], { type: 'text/csv' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = `inventory-${new Date().toISOString().split('T')[0]}.csv`;
		a.click();
		URL.revokeObjectURL(url);
	}

	function importFromCSV() {
		const input = document.createElement('input');
		input.type = 'file';
		input.accept = '.csv';
		input.onchange = (e) => {
			const file = (e.target as HTMLInputElement).files?.[0];
			if (!file) return;

			const reader = new FileReader();
			reader.onload = (event) => {
				const csv = event.target?.result as string;
				const lines = csv.split('\n').filter(l => l.trim());
				if (lines.length < 2) {
					alert('CSV file is empty or invalid');
					return;
				}

				// Skip header row
				const dataLines = lines.slice(1);
				const imported: EquipmentItem[] = [];

				for (const line of dataLines) {
					const values = line.match(/("([^"]|"")*"|[^,]+)/g)?.map(v => v.replace(/^"|"$/g, '').replace(/""/g, '"')) || [];
					if (values.length >= 7) {
						imported.push({
							id: Date.now().toString() + Math.random().toString(36).substr(2, 9),
							name: values[0] || '',
							category: values[1] || '',
							location: values[2] || '',
							serialModel: values[3] || '',
							condition: values[4] || '',
							purchaseDate: values[5] || '',
							value: values[6] || '',
							notes: values[7] || ''
						});
					}
				}

				items = [...items, ...imported];
				saveItems();
				alert(`Imported ${imported.length} items`);
			};
			reader.readAsText(file);
		};
		input.click();
	}

	// Filtered items for display
	let filteredItems = $derived(
		items.filter(item => {
			const matchesSearch = searchTerm === '' ||
				item.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
				item.location.toLowerCase().includes(searchTerm.toLowerCase()) ||
				item.serialModel.toLowerCase().includes(searchTerm.toLowerCase());
			const matchesCategory = filterCategory === '' || item.category === filterCategory;
			return matchesSearch && matchesCategory;
		})
	);
</script>

<svelte:head>
	<title>Inventory — Radix</title>
</svelte:head>

<Box style="padding: 1.5rem; max-width: 1400px; margin: 0 auto;">
	<Heading level={1}>📦 Equipment Inventory</Heading>

	<!-- Add Item Form -->
	<Card style="margin-top: 1.5rem;">
		<Heading level={2}>Add Equipment</Heading>
		<div class="form-grid">
			<Input label="Equipment Name *" bind:value={name} placeholder="e.g., John Deere Tractor" />
			<Select label="Category" bind:value={category} options={categories} placeholder="Select category" />
			<Input label="Location" bind:value={location} placeholder="e.g., Barn, Workshop" />
			<Input label="Serial/Model #" bind:value={serialModel} placeholder="Serial or model number" />
			<Select label="Condition" bind:value={condition} options={conditions} placeholder="Select condition" />
			<Input label="Purchase Date" type="date" bind:value={purchaseDate} />
			<Input label="Value ($)" bind:value={value} placeholder="e.g., 15000" />
		</div>
		<TextArea label="Notes" bind:value={notes} placeholder="Additional details..." rows={3} />
		<div class="form-actions">
			<Button onclick={addItem}>Add Equipment</Button>
		</div>
	</Card>

	<!-- Inventory List -->
	<Card style="margin-top: 1.5rem;">
		<div class="list-header">
			<Heading level={2}>Inventory ({filteredItems.length} items)</Heading>
			<div class="list-actions">
				<Button onclick={exportToCSV} disabled={items.length === 0}>📥 Export CSV</Button>
				<Button onclick={importFromCSV}>📤 Import CSV</Button>
			</div>
		</div>

		<div class="filters">
			<Input bind:value={searchTerm} placeholder="🔍 Search by name, location, or serial..." />
			<Select bind:value={filterCategory} options={[{ value: '', label: 'All Categories' }, ...categories]} />
		</div>

		{#if filteredItems.length === 0}
			<div class="empty-state">
				{#if items.length === 0}
					<p>No equipment added yet. Use the form above to get started.</p>
				{:else}
					<p>No items match your filters.</p>
				{/if}
			</div>
		{:else}
			<div class="table-wrapper">
				<table class="inventory-table">
					<thead>
						<tr>
							<th>Name</th>
							<th>Category</th>
							<th>Location</th>
							<th>Serial/Model</th>
							<th>Condition</th>
							<th>Purchase Date</th>
							<th>Value</th>
							<th>Notes</th>
							<th>Actions</th>
						</tr>
					</thead>
					<tbody>
						{#each filteredItems as item (item.id)}
							<tr>
								<td class="item-name">{item.name}</td>
								<td>{item.category}</td>
								<td>{item.location}</td>
								<td>{item.serialModel}</td>
								<td>{item.condition}</td>
								<td>{item.purchaseDate}</td>
								<td>{item.value ? `$${item.value}` : ''}</td>
								<td class="item-notes">{item.notes}</td>
								<td>
									<button class="delete-btn" onclick={() => deleteItem(item.id)}>🗑️</button>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	</Card>
</Box>

<style>
	.form-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
		gap: 1rem;
		margin-top: 1rem;
	}

	.form-actions {
		margin-top: 1rem;
		display: flex;
		gap: 0.5rem;
	}

	.list-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex-wrap: wrap;
		gap: 1rem;
	}

	.list-actions {
		display: flex;
		gap: 0.5rem;
	}

	.filters {
		display: grid;
		grid-template-columns: 2fr 1fr;
		gap: 1rem;
		margin: 1rem 0;
	}

	.empty-state {
		text-align: center;
		padding: 3rem 1rem;
		color: var(--color-text-muted);
	}

	.table-wrapper {
		overflow-x: auto;
		margin-top: 1rem;
	}

	.inventory-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.85rem;
	}

	.inventory-table th {
		background: var(--color-hover);
		padding: 0.75rem;
		text-align: left;
		font-weight: 600;
		border-bottom: 2px solid var(--color-border);
		white-space: nowrap;
	}

	.inventory-table td {
		padding: 0.75rem;
		border-bottom: 1px solid var(--color-border);
	}

	.inventory-table tbody tr:hover {
		background: var(--color-hover);
	}

	.item-name {
		font-weight: 500;
	}

	.item-notes {
		max-width: 200px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.delete-btn {
		background: none;
		border: none;
		cursor: pointer;
		font-size: 1.1rem;
		padding: 0.25rem;
		opacity: 0.6;
		transition: opacity 0.2s;
	}

	.delete-btn:hover {
		opacity: 1;
	}
</style>
