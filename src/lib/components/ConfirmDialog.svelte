<!--
  ConfirmDialog — native <dialog> element wrapper.
-->
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
		open = $bindable(),
		title,
		message,
		confirmLabel = 'Confirm',
		cancelLabel = 'Cancel',
		onConfirm,
		onCancel
	}: Props = $props();

	let dialogEl = $state<HTMLDialogElement | undefined>(undefined);

	$effect(() => {
		if (!dialogEl) return;
		if (open) {
			if (!dialogEl.open) dialogEl.showModal();
		} else {
			if (dialogEl.open) dialogEl.close();
		}
	});

	function handleClose() {
		// `close` fires for both programmatic close and Escape key.
		// Only treat it as a cancel if open is still true (Escape key case);
		// button clicks update `open` via the parent before this fires.
		if (open) {
			open = false;
			onCancel();
		}
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dialogEl} onclose={handleClose} class="dialog">
	<div class="dialog-inner">
		<h2 class="dialog-title">{title}</h2>
		<p class="confirm-message">{message}</p>
		<div class="confirm-actions">
			<button class="btn-secondary" onclick={onCancel}>{cancelLabel}</button>
			<button class="btn-primary" onclick={onConfirm}>{confirmLabel}</button>
		</div>
	</div>
</dialog>

<style>
	.dialog {
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: var(--color-surface);
		color: var(--color-text);
		padding: 0;
		min-width: 320px;
		max-width: 480px;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
	}

	.dialog::backdrop {
		background: rgba(0, 0, 0, 0.5);
	}

	.dialog-inner {
		padding: 20px 24px;
	}

	.dialog-title {
		margin: 0 0 8px;
		font-size: 1.1rem;
		color: var(--color-text);
	}

	.confirm-message {
		margin: 0 0 20px;
		color: var(--color-text-muted, #8b92a5);
		font-size: 0.9rem;
		line-height: 1.5;
	}

	.confirm-actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
	}

	.btn-primary {
		padding: 7px 16px;
		border-radius: 6px;
		border: none;
		background: var(--color-accent);
		color: #fff;
		font-size: 0.875rem;
		cursor: pointer;
		font-weight: 500;
	}

	.btn-primary:hover { opacity: 0.9; }

	.btn-secondary {
		padding: 7px 16px;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-text);
		font-size: 0.875rem;
		cursor: pointer;
	}

	.btn-secondary:hover { background: var(--color-hover); }
</style>
