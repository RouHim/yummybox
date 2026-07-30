<script lang="ts">
	import { fade, scale } from 'svelte/transition';
	import { tierDuration } from '$lib/motion';
import { focusTrap } from '$lib/focusTrap';

	let {
		open,
		title,
		message,
		confirmLabel,
		cancelLabel,
		onconfirm,
		oncancel,
	}: {
		open: boolean;
		title: string;
		message: string;
		confirmLabel: string;
		cancelLabel: string;
		onconfirm: () => void;
		oncancel: () => void;
	} = $props();

	let dialogEl = $state<HTMLDivElement | null>(null);

</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="confirm-dialog glass--strong"
		role="alertdialog"
		aria-modal="true"
		aria-label={title}
		tabindex="-1"
		bind:this={dialogEl}
		transition:fade={{ duration: tierDuration(200) }}
		onclick={oncancel}
		onkeydown={(e) => {
			if (e.key === 'Escape') oncancel();
		}}
	>
		<div
			class="confirm-dialog__panel glass--strong"
			in:scale={{ duration: tierDuration(250), start: 0.95 }}
			use:focusTrap
			onclick={(e) => e.stopPropagation()}
			onkeydown={() => {}}
		>
			<h2 class="confirm-dialog__title">{title}</h2>
			<p class="confirm-dialog__message">{message}</p>
			<div class="confirm-dialog__actions">
				<button type="button" class="btn btn--ghost" onclick={oncancel}>{cancelLabel}</button>
				<button type="button" class="btn btn--primary" onclick={onconfirm}>{confirmLabel}</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.confirm-dialog {
		position: fixed;
		inset: 0;
		z-index: 1001;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--glass-scrim);
		padding: var(--space-4);
	}
	.confirm-dialog__panel {
		max-width: 420px;
		width: 100%;
		padding: var(--space-6);
		border-radius: var(--radius-lg);
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.confirm-dialog__title {
		margin: 0;
		font-size: var(--text-xl);
		font-weight: var(--weight-semibold);
		color: var(--color-text);
	}
	.confirm-dialog__message {
		margin: 0;
		color: var(--color-text-secondary);
	}
	.confirm-dialog__actions {
		display: flex;
		gap: var(--space-2);
		justify-content: flex-end;
	}
</style>
