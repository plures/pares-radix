<script lang="ts">
	import type { CommandPaletteProps, CommandItem } from './types.js';

	let {
		open = $bindable(false),
		commands = [],
		onClose
	}: CommandPaletteProps = $props();

	let query = $state('');
	let selectedIndex = $state(0);
	let dialogEl: HTMLDialogElement | undefined = $state();
	let inputEl: HTMLInputElement | undefined = $state();

	let filtered = $derived(
		query.trim() === ''
			? commands
			: commands.filter((c) =>
					c.label.toLowerCase().includes(query.toLowerCase())
				)
	);

	$effect(() => {
		if (!dialogEl) return;
		if (open) {
			if (!dialogEl.open) dialogEl.showModal();
			// Focus the search input on next tick
			setTimeout(() => inputEl?.focus(), 0);
		} else {
			if (dialogEl.open) dialogEl.close();
		}
	});

	$effect(() => {
		// Reset selection whenever the filtered list changes.
		// Reading filtered.length establishes the reactive dependency.
		selectedIndex = filtered.length > 0 ? 0 : 0;
	});

	function close() {
		open = false;
		query = '';
		onClose?.();
	}

	function select(item: CommandItem) {
		item.action();
		close();
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			close();
		} else if (e.key === 'ArrowDown') {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, filtered.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
		} else if (e.key === 'Enter' && filtered[selectedIndex]) {
			select(filtered[selectedIndex]);
		}
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<dialog
	bind:this={dialogEl}
	class="command-palette"
	onclose={close}
	onkeydown={handleKeydown}
	aria-label="Command palette"
>
	<div class="palette-inner">
		<div class="search-row">
			<span class="search-icon" aria-hidden="true">⌘</span>
			<input
				bind:this={inputEl}
				bind:value={query}
				class="search-input"
				type="search"
				placeholder="Type a command…"
				aria-label="Search commands"
				autocomplete="off"
			/>
		</div>

		<ul class="command-list" role="listbox" aria-label="Commands">
			{#each filtered as item, i (item.id)}
				<li
					role="option"
					aria-selected={i === selectedIndex}
					class="command-item"
					class:selected={i === selectedIndex}
				>
					<button
						class="command-btn"
						onclick={() => select(item)}
						onmouseenter={() => (selectedIndex = i)}
					>
						{#if item.icon}
							<span class="cmd-icon" aria-hidden="true">{item.icon}</span>
						{/if}
						<span class="cmd-label">{item.label}</span>
					</button>
				</li>
			{:else}
				<li class="empty-state" role="option" aria-selected="false">No commands found</li>
			{/each}
		</ul>
	</div>
</dialog>

<style>
	.command-palette {
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: var(--color-surface);
		color: var(--color-text);
		padding: 0;
		width: 480px;
		max-width: calc(100vw - 32px);
		max-height: 420px;
		overflow: hidden;
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.35);
		top: 15vh;
		margin: 0 auto;
	}

	.command-palette::backdrop {
		background: rgba(0, 0, 0, 0.45);
	}

	.palette-inner {
		display: flex;
		flex-direction: column;
		max-height: 420px;
	}

	.search-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 12px 16px;
		border-bottom: 1px solid var(--color-border);
	}

	.search-icon {
		font-size: 1rem;
		color: var(--color-text-muted);
		flex-shrink: 0;
	}

	.search-input {
		flex: 1;
		background: transparent;
		border: none;
		outline: none;
		font-size: 1rem;
		color: var(--color-text);
		padding: 0;
	}

	.search-input::placeholder { color: var(--color-text-muted); }

	.command-list {
		list-style: none;
		margin: 0;
		padding: 6px;
		overflow-y: auto;
		flex: 1;
	}

	.command-item { border-radius: 6px; }

	.command-item.selected { background: var(--color-accent-bg); }

	.command-btn {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		background: transparent;
		border: none;
		cursor: pointer;
		padding: 9px 12px;
		border-radius: 6px;
		color: var(--color-text);
		font-size: 0.9rem;
		text-align: left;
	}

	.command-btn:hover { background: var(--color-hover); }

	.command-item.selected .command-btn { color: var(--color-accent); }

	.cmd-icon { font-size: 1.1rem; width: 22px; text-align: center; flex-shrink: 0; }
	.cmd-label { flex: 1; }

	.empty-state {
		padding: 24px;
		text-align: center;
		color: var(--color-text-muted);
		font-size: 0.9rem;
	}
</style>
