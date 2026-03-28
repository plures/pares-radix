<script lang="ts">
	interface Props {
		open: boolean;
		title: string;
		message: string;
		confirmLabel?: string;
		cancelLabel?: string;
		onConfirm: () => void;
		onCancel: () => void;
	}

	let {
		open,
		title,
		message,
		confirmLabel = 'Confirm',
		cancelLabel = 'Cancel',
		onConfirm,
		onCancel
	}: Props = $props();
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<div class="overlay" role="presentation" onclick={onCancel}>
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<div class="dialog" role="alertdialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
			<h3>{title}</h3>
			<p>{message}</p>
			<div class="actions">
				<button class="btn secondary" onclick={onCancel}>{cancelLabel}</button>
				<button class="btn danger" onclick={onConfirm}>{confirmLabel}</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 200;
	}

	.dialog {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		padding: 24px;
		max-width: 400px;
		width: 90%;
	}

	.dialog h3 {
		margin: 0 0 8px;
		color: var(--color-text);
	}

	.dialog p {
		margin: 0 0 20px;
		color: var(--color-text-muted);
		font-size: 0.9rem;
	}

	.actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
	}

	.btn {
		padding: 8px 16px;
		border-radius: 6px;
		border: none;
		cursor: pointer;
		font-size: 0.85rem;
		font-weight: 500;
	}

	.btn.secondary {
		background: var(--color-hover);
		color: var(--color-text);
	}

	.btn.danger {
		background: var(--color-danger);
		color: white;
	}
</style>
