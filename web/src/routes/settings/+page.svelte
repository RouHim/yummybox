<script lang="ts">
	import LlmConfigPicker from '$lib/components/LlmConfigPicker.svelte';
	import { checkBringStatus, getSettings, updateSettings } from '$lib/api';
	import {
		SettingsCommitter,
		commitStatusLabelKey,
		secretSourceLabelKey,
		type SecretState,
		type SettingsSnapshot,
		type ValueSource,
	} from '$lib/settings.svelte';
	import { t } from '$lib/i18n';

	let provider = $state('');
	let providerName = $state('');
	let model = $state('');
	let providersReady = $state(true);
	let aiConfigured = $state(false);

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
	<header class="settings-hero glass">
		<h1 class="settings-hero__title">{t('settingsTitle')}</h1>
		<p class="settings-hero__sub">{t('settingsIntro')}</p>
	</header>

	<section class="settings-card glass">
		<h2 class="settings-card__title">{t('settingsAiTitle')}</h2>
		<p class="settings-card__intro">{t('settingsAiIntro')}</p>
		<LlmConfigPicker bind:provider bind:providerName bind:model bind:providersReady bind:configured={aiConfigured} />
	</section>

	<section class="settings-card glass">
		<h2 class="settings-card__title">{t('settingsBringTitle')}</h2>
		<p class="settings-card__intro">{t('settingsBringIntro')}</p>

		<label class="import-field">
			<span>{t('settingsBringEmailLabel')}</span>
			<input type="email" bind:value={bringEmail} onchange={onBringEmailChange}
				onkeydown={(e) => onEnter(e, onBringEmailChange)} />
		</label>
		<p class="settings-state">{t(secretSourceLabelKey(bringEmailSource))}</p>

		<label class="import-field">
			<span>{t('settingsBringPasswordLabel')}</span>
			<input type="password" bind:value={bringPasswordInput}
				placeholder={t('settingsSecretReplacePlaceholder')}
				onchange={onBringPasswordChange}
				onkeydown={(e) => onEnter(e, onBringPasswordChange)} />
		</label>
		<p class="settings-state">
			{t(secretSourceLabelKey(bringPasswordState.source))}
			{#if bringPasswordState.source === 'settings'}
				<button type="button" class="btn btn--ghost" onclick={onClearBringPassword}>
					{t('settingsSecretClear')}
				</button>
			{/if}
		</p>

		<p class="settings-state settings-state--status" role="status">
			{commitStatusLabelKey(committer.state.status) ? t(commitStatusLabelKey(committer.state.status)!) : ''}
		</p>
		{#if committer.state.status === 'error'}
			<p class="form-error" role="alert">{committer.state.error}</p>
		{/if}

		{#if bringStatus === 'checking'}
			<p class="settings-state" role="status">{t('settingsBringChecking')}</p>
		{:else if bringStatus === 'connected'}
			<p class="settings-state" role="status">{t('settingsBringConnected')}</p>
		{/if}
		{#if bringStatusError}
			<p class="form-error" role="alert">{bringStatusError}</p>
		{/if}
	</section>

	<p class="settings-note">{t('settingsSecurityNote')}</p>
</main>

<style>
	.settings-page {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding: var(--space-4) var(--space-3) var(--space-6);
	}
	.settings-hero {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-5) var(--space-4);
	}
	.settings-hero__title {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-2xl);
		color: var(--color-primary);
	}
	.settings-hero__sub {
		margin: 0;
		color: var(--color-text-secondary);
	}
	.settings-card {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-4);
	}
	.settings-card__title {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-lg);
	}
	.settings-card__intro {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
	.settings-state {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-muted);
	}
	.settings-state--status {
		min-height: 1.2em;
	}
	.settings-note {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-muted);
	}
</style>
