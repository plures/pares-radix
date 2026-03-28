<!--
  ConfirmDialog — thin wrapper around design-dojo's Dialog.
-->
<script lang="ts">
	import { Dialog, Button } from '@plures/design-dojo';

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
</script>

<Dialog bind:open {title} onclose={onCancel}>
	<p class="confirm-message">{message}</p>
	{#snippet footer()}
		<div class="confirm-actions">
			<Button variant="secondary" onclick={onCancel}>{cancelLabel}</Button>
			<Button variant="primary" onclick={onConfirm}>{confirmLabel}</Button>
		</div>
	{/snippet}
</Dialog>

<style>
	.confirm-message {
		margin: 0;
		color: var(--color-text-muted, #8b92a5);
		font-size: 0.9rem;
		line-height: 1.5;
	}

	.confirm-actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
	}
</style>
