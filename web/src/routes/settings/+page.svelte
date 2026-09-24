<script lang="ts">
	import LlmConfigPicker from '$lib/components/LlmConfigPicker.svelte';
	import { checkBringStatus, getSettings, updateSettings } from '$lib/api';
	import {
		SettingsCommitter,
		commitStatusChipClass,
		commitStatusLabelKey,
		secretSourceLabelKey,
		type SecretState,
		type SettingsSnapshot,
		type ValueSource,
	} from '$lib/settings.svelte';
	import { t } from '$lib/i18n';
	import type { TranslationKey } from '$lib/i18n/types';
	import Icon from '$lib/Icon.svelte';

	let provider = $state('');
	let providerName = $state('');
	let model = $state('');
	let providersReady = $state(true);
	let aiConfigured = $state(false);
	// Distinguishes "no provider configured" from "configuration not read yet".
	let aiLoaded = $state(false);

	let bringEmail = $state('');
	let bringEmailSource = $state<ValueSource>('none');
	let bringPasswordInput = $state('');
	let bringPasswordState = $state<SecretState>({ set: false, source: 'none' });
	let bringStatus = $state<'idle' | 'checking' | 'connected' | 'error'>('idle');
	let bringStatusError = $state<string | null>(null);

	// Snapshots are applied in request order. The mount-time load captures its
	// sequence number before the request starts, so a stale response that lands
	// after a newer commit response is discarded instead of overwriting the
	// state that commit just showed (the commit response carries a higher
	// number because its request started later).
	let appliedSeq = 0;
	let requestSeq = 0;

	// Shared by the initial load and by every commit response: the snapshot is
	// the only source of truth for where a value comes from.
	function applyBringSnapshot(snapshot: SettingsSnapshot, seq = ++requestSeq) {
		if (seq <= appliedSeq) return;
		appliedSeq = seq;
		bringEmailSource = snapshot.bring.emailSource;
		bringPasswordState = snapshot.bring.password;
		// Only refill the email when the field is empty: that is the case after
		// a clear, where the environment value becomes effective again. A value
		// the user is typing is never overwritten by an older response.
		if (!bringEmail.trim()) bringEmail = snapshot.bring.email;
	}

	// Commits are serialized, exactly like in the AI picker: the newest value
	// always wins, regardless of how long an earlier request takes.
	const committer = new SettingsCommitter(updateSettings, applyBringSnapshot);

	// Section status: the one thing a user opens this page to check.
	let aiChip = $derived(
		aiConfigured
			? { key: 'settingsStatusReady' as TranslationKey, chipClass: 'state-chip--ok', dot: true }
			: { key: 'settingsStatusNotConfigured' as TranslationKey, chipClass: 'state-chip--neutral', dot: false },
	);

	let bringChip = $derived.by(() => {
		if (bringStatus === 'checking') {
			return { key: 'settingsStatusChecking' as TranslationKey, chipClass: 'state-chip--neutral', dot: false };
		}
		if (bringStatus === 'connected') {
			return { key: 'settingsStatusConnected' as TranslationKey, chipClass: 'state-chip--ok', dot: true };
		}
		return {
			key: 'settingsStatusNotConnected' as TranslationKey,
			chipClass: bringStatus === 'error' ? 'state-chip--alert' : 'state-chip--neutral',
			dot: false,
		};
	});

	async function refreshBringStatus() {
		bringStatus = 'checking';
		bringStatusError = null;
		try {
			const res = await checkBringStatus();
			if (!res.configured) {
				bringStatus = 'idle';
			} else if (res.connected) {
				bringStatus = 'connected';
			} else {
				bringStatus = 'error';
				bringStatusError = res.error;
			}
		} catch (err) {
			bringStatus = 'error';
			bringStatusError = err instanceof Error ? err.message : String(err);
		}
	}

	function onBringEmailChange() {
		committer
			.commit({ bring: { email: bringEmail.trim() ? bringEmail : null } })
			.then(refreshBringStatus);
	}

	function onBringPasswordChange() {
		const value = bringPasswordInput;
		if (!value) return;
		committer.commit({ bring: { password: value } }).then(() => {
			if (bringPasswordInput === value) bringPasswordInput = '';
			refreshBringStatus();
		});
	}

	function onClearBringPassword() {
		bringPasswordInput = '';
		committer.commit({ bring: { password: null } }).then(refreshBringStatus);
	}

	function onEnter(event: KeyboardEvent, commitField: () => void) {
		if (event.key !== 'Enter') return;
		event.preventDefault();
		commitField();
	}

	$effect(() => {
		refreshBringStatus();
		const seq = ++requestSeq;
		getSettings()
			.then((snapshot) => applyBringSnapshot(snapshot, seq))
			.catch((err) => {
				if (seq <= appliedSeq) return;
				bringStatus = 'error';
				bringStatusError = err instanceof Error ? err.message : String(err);
			});
	});
</script>

<main class="settings-page">
	<header class="settings-hero glass glass--strong">
		<h1 class="settings-hero__title">{t('settingsTitle')}</h1>
	</header>

	<section class="settings-card glass glass--strong">
		<header class="settings-card__header">
			<div class="settings-card__head">
				<span class="settings-card__icon" aria-hidden="true"><Icon name="sparkles" size={16} /></span>
				<h2 class="settings-card__title">{t('settingsAiTitle')}</h2>
				{#if aiLoaded}
					<span class="state-chip {aiChip.chipClass}">
						{#if aiChip.dot}<span class="state-chip__dot" aria-hidden="true"></span>{/if}
						{t(aiChip.key)}
					</span>
				{/if}
			</div>
			<p class="settings-card__intro">{t('settingsAiIntro')}</p>
		</header>

		<LlmConfigPicker
			variant="plain"
			bind:provider
			bind:providerName
			bind:model
			bind:providersReady
			bind:configured={aiConfigured}
			bind:loaded={aiLoaded}
		/>
	</section>

	<section class="settings-card glass glass--strong">
		<header class="settings-card__header">
			<div class="settings-card__head">
				<span class="settings-card__icon" aria-hidden="true"><Icon name="shopping-bag" size={16} /></span>
				<h2 class="settings-card__title">{t('settingsBringTitle')}</h2>
				<span class="state-chip {bringChip.chipClass}">
					{#if bringChip.dot}<span class="state-chip__dot" aria-hidden="true"></span>{/if}
					{t(bringChip.key)}
				</span>
			</div>
			<p class="settings-card__intro">{t('settingsBringIntro')}</p>
		</header>

		<div class="field">
			<div class="field__head">
				<label class="field__label" for="bring-email">{t('settingsBringEmailLabel')}</label>
				<span class="state-chip state-chip--neutral">{t(secretSourceLabelKey(bringEmailSource))}</span>
			</div>
			<input id="bring-email" type="email" bind:value={bringEmail} onchange={onBringEmailChange}
				onkeydown={(e) => onEnter(e, onBringEmailChange)} />
		</div>

		<div class="field">
			<div class="field__head">
				<label class="field__label" for="bring-password">{t('settingsBringPasswordLabel')}</label>
				<span class="state-chip state-chip--neutral">{t(secretSourceLabelKey(bringPasswordState.source))}</span>
				{#if bringPasswordState.source === 'settings'}
					<button type="button" class="btn btn--ghost btn--compact" onclick={onClearBringPassword}>
						{t('settingsSecretClear')}
					</button>
				{/if}
			</div>
			<input id="bring-password" type="password" bind:value={bringPasswordInput}
				placeholder={bringPasswordState.set || bringPasswordState.source === 'environment'
					? t('settingsSecretReplacePlaceholder')
					: t('settingsPasswordPlaceholder')}
				onchange={onBringPasswordChange}
				onkeydown={(e) => onEnter(e, onBringPasswordChange)} />
		</div>

		{#if commitStatusLabelKey(committer.state.status)}
			<p class="settings-commit" role="status">
				<span class="state-chip {commitStatusChipClass(committer.state.status)}">
					{t(commitStatusLabelKey(committer.state.status)!)}
				</span>
			</p>
		{/if}
		{#if committer.state.status === 'error'}
			<p class="form-error" role="alert">{committer.state.error}</p>
		{/if}
		{#if bringStatusError}
			<p class="form-error" role="alert">{bringStatusError}</p>
		{/if}
	</section>

	<aside class="settings-note" role="note">
		<span class="settings-note__icon" aria-hidden="true"><Icon name="circle-alert" size={16} /></span>
		<p class="settings-note__text">{t('settingsSecurityNote')}</p>
	</aside>
</main>

<style>
	.settings-page {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		max-width: min(100% - 2rem, 34rem);
		margin: 0 auto;
		padding: var(--space-2) 0 var(--space-6);
	}

	/* Page entry: reveals the page in reading order. */
	@media (prefers-reduced-motion: no-preference) {
		.settings-page > * {
			animation: settings-rise var(--motion-enter) both;
		}
		.settings-page > :nth-child(2) {
			animation-delay: 60ms;
		}
		.settings-page > :nth-child(3) {
			animation-delay: 120ms;
		}
		.settings-page > :nth-child(4) {
			animation-delay: 180ms;
		}
	}
	@keyframes settings-rise {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	.settings-hero {
		padding: var(--space-4) var(--space-5);
		border-radius: var(--radius-lg);
	}
	.settings-hero__title {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-2xl);
		font-weight: var(--weight-semibold);
		line-height: 1.15;
		color: var(--color-primary);
	}

	.settings-card {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding: var(--space-5);
		border-radius: var(--radius-lg);
	}
	/* Section identity above, its configuration below. */
	.settings-card__header {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding-bottom: var(--space-3);
		border-bottom: 1px solid var(--color-border);
	}
	.settings-card__head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}
	.settings-card__icon {
		display: inline-flex;
		color: var(--color-primary);
	}
	.settings-card__title {
		margin: 0 auto 0 0;
		font-family: var(--font-display);
		font-size: var(--text-xl);
		font-weight: var(--weight-semibold);
		line-height: 1.2;
	}
	.settings-card__intro {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
	.settings-commit {
		margin: 0;
	}

	.settings-note {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
		background: var(--color-surface-2);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
	}
	.settings-note__icon {
		display: inline-flex;
		margin-top: 2px;
		color: var(--color-primary);
	}
	.settings-note__text {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
</style>
