<!--
  ConfirmDialog — native <dialog> element, no external dependencies.
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

	let dialogEl: HTMLDialogElement | undefined = $state();
	let confirming = $state(false);

	$effect(() => {
		if (!dialogEl) return;
		if (open) {
			if (!dialogEl.open) dialogEl.showModal();
		} else {
			if (dialogEl.open) dialogEl.close();
		}
	});

	function handleClose() {
		open = false;
		if (!confirming) onCancel();
		confirming = false;
	}

	function handleConfirm() {
		confirming = true;
		open = false;
		onConfirm();
	}
</script>

<dialog
	bind:this={dialogEl}
	class="confirm-dialog"
	onclose={handleClose}
>
	<div class="dialog-inner">
		<h2 class="dialog-title">{title}</h2>
		<p class="confirm-message">{message}</p>
		<div class="confirm-actions">
			<button class="btn secondary" onclick={onCancel}>{cancelLabel}</button>
			<button class="btn primary" onclick={handleConfirm}>{confirmLabel}</button>
		</div>
	</div>
</dialog>

<style>
	.confirm-dialog {
		border: 1px solid var(--color-border, #2d3140);
		border-radius: 10px;
		background: var(--color-surface, #1a1d27);
		color: var(--color-text, #e2e5eb);
		padding: 0;
		min-width: 320px;
		max-width: 480px;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
	}

	.confirm-dialog::backdrop {
		background: rgba(0, 0, 0, 0.5);
	}

	.dialog-inner {
		padding: 24px;
	}

	.dialog-title {
		margin: 0 0 12px;
		font-size: 1.05rem;
		color: var(--color-text, #e2e5eb);
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

	.btn {
		padding: 7px 16px;
		border-radius: 6px;
		font-size: 0.85rem;
		cursor: pointer;
		border: 1px solid var(--color-border, #2d3140);
		font-weight: 500;
		transition: background 0.12s;
	}

	.btn.primary {
		background: var(--color-accent, #6366f1);
		color: #fff;
		border-color: transparent;
	}

	.btn.primary:hover { filter: brightness(1.1); }

	.btn.secondary {
		background: var(--color-surface, #1a1d27);
		color: var(--color-text, #e2e5eb);
	}

	.btn.secondary:hover { background: var(--color-hover, rgba(255,255,255,0.05)); }
</style>
