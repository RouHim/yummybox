<script lang="ts">
	import { listLlmProviders, listLlmModels, getSettings, updateSettings, ApiError } from '$lib/api';
	import {
		SettingsCommitter,
		commitStatusChipClass,
		commitStored,
		isAiConfigured,
		commitStatusLabelKey,
		providerChangePatch,
		providerChanged,
		secretSourceLabelKey,
		type CommitStatus,
		type SecretState,
		type SettingsPatch,
		type SettingsSnapshot,
	} from '$lib/settings.svelte';
	import Icon from '$lib/Icon.svelte';
	import { t } from '$lib/i18n';
	import type { LlmProviderInfo } from '$lib/types';

	let {
		provider = $bindable(''),
		providerName = $bindable(''),
		model = $bindable(''),
		disabled = false,
		providersReady = $bindable(true),
		configured = $bindable(false),
		loaded = $bindable(false),
		variant = 'panel',
		onsettle,
	}: {
		provider?: string;
		providerName?: string;
		model?: string;
		disabled?: boolean;
		providersReady?: boolean;
		configured?: boolean;
		/** True once the stored configuration has been read, so a caller can
		 *  tell "not configured" apart from "not known yet". */
		loaded?: boolean;
		/** `panel` draws its own surface (import dialogs); `plain` sits in a card that already has one. */
		variant?: 'panel' | 'plain';
		/** Handed the same wait the `settle` export offers, so a caller keeps it
		 *  when it destroys this instance (a tab switch): a commit still in
		 *  flight stays awaitable through the write it issued instead of being
		 *  lost with the component that issued it. */
		onsettle?: (settle: () => Promise<void>) => void;
	} = $props();

	let llmProviders = $state<LlmProviderInfo[]>([]);
	let llmProvidersLoading = $state(false);
	let llmProvidersLoaded = $state(false);
	let llmModels: string[] = $state([]);
	let llmModelsLoading = $state(false);
	let llmModelsError = $state<string | null>(null);
	// Raw cause behind the actionable sentence: shown only on request, never
	// as the first thing the user reads.
	let llmModelsErrorDetail = $state<string | null>(null);
	let customBaseUrl = $state('');
	let apiKeyInput = $state('');
	let apiKeyState = $state<SecretState>({ set: false, source: 'none' });
	let restored = $state(false);
	// Whether the mount-time read of the stored configuration has settled
	// (succeeded or failed). The model listing waits for it, so a remount lists
	// the provider's models once, on the state the answer establishes.
	let settingsReadSettled = $state(false);
	// A failed settings read leaves `loaded` false: the caller must keep
	// treating the configuration as unknown, never as unconfigured.
	let settingsLoadFailed = $state(false);
	// Provider whose models were already loaded this mount; ensures a fresh
	// model load after the component remounts (collapse/expand, tab switch).
	let modelsLoadedFor: string | null = null;
	// Monotonic sequence for model-list requests: a slow earlier response must
	// not overwrite the models of a newer provider switch.
	let modelsRequestSeq = 0;
	// The same discipline for settings snapshots: the mount-time read takes its
	// number before the request starts, and a commit takes one when it is
	// issued, so a read that started earlier is superseded from that moment on
	// instead of refilling a field the commit cleared.
	let appliedSnapshotSeq = 0;
	let snapshotRequestSeq = 0;
	// Commits are serialized, so their answers arrive in the order they were
	// issued. Counting both ends tells an answer apart from the newest one
	// issued: an answer that is not the last one predates a commit the user has
	// already made, and filling an emptied field from it would restore the value
	// that newer edit removed. Both ends count every commit, a rejected one
	// included: a rejection is answered like any other write, so the counters
	// keep their relation after a failed commit instead of drifting apart.
	let commitIssueSeq = 0;
	let commitAnswerSeq = 0;
	// Whether the dependent fields were already empty when the latest commit
	// was issued. An answer may fill a field only when it was empty then too: a
	// field the user emptied while the commit was in flight was not empty at
	// issue, and filling it would restore the value that edit removed.
	let modelEmptyAtCommit = false;
	let baseUrlEmptyAtCommit = false;

	/**
	 * Whether a read numbered `seq` is still the newest request issued. A
	 * commit issued after the read takes a higher number, and that read's body -
	 * the state before the commit - still carries the values the commit
	 * removed, so only the newest answer may fill a field.
	 */
	function isNewest(seq: number): boolean {
		return seq === snapshotRequestSeq;
	}

	// The provider the server last confirmed, or `null` while the stored
	// configuration has not been read (or the read failed). A selection that
	// matches it changed nothing and must not delete its dependent values.
	// Reactive: the remount effect and the rejected-commit retry read it, and
	// both have to see the provider an answer confirmed.
	let appliedProvider = $state<string | null>(null);

	/** Server-owned snapshot fields, applied from the newest answer only. */
	function applySnapshot(snapshot: SettingsSnapshot, seq: number) {
		if (seq <= appliedSnapshotSeq) return;
		appliedSnapshotSeq = seq;
		appliedProvider = snapshot.ai.provider;
		apiKeyState = snapshot.ai.apiKey;
		// The answer carries the stored model and endpoint: fill them in while
		// the user has not typed one, so a selection that changed nothing keeps
		// showing the values it would otherwise have deleted. Only an answer of
		// the commit last issued may fill: the body of an earlier commit already
		// reports the values a newer commit removed. That rule is the counters
		// below: this apply callback runs before `commit` counts the answer, so
		// the commit being applied is the next answer and the two agree exactly
		// when it is the last one issued. Its field must also have been empty at
		// issue: a field the user emptied while the commit was in flight is a
		// newer local edit, and filling it here would undo that edit. The
		// snapshot must also report the provider the select shows, exactly as
		// the mount read requires: an answer can carry another provider's
		// values - a rejected switch leaves the server holding the previous
		// provider while the select shows the new one - and filling them would
		// put a model into a control for a provider the server does not have it
		// for, which the next commit would then store against the stored
		// provider.
		if (commitAnswerSeq + 1 === commitIssueSeq && snapshot.ai.provider === provider) {
			if (!model && modelEmptyAtCommit) model = snapshot.ai.model;
			if (!customBaseUrl && baseUrlEmptyAtCommit) customBaseUrl = snapshot.ai.customBaseUrl;
		}
		configured = isAiConfigured(snapshot);
		loaded = true;
		// An applied snapshot proves the server answered, so a failure reported
		// by an earlier read must not keep labelling the form as unloaded.
		settingsLoadFailed = false;
	}

	// Commits are serialized: a settings change may only be followed by the
	// next one once the previous request has been answered, so a slow
	// provider commit can never land after a newer model commit.
	const committer = new SettingsCommitter(updateSettings, (snapshot) => {
		applySnapshot(snapshot, ++snapshotRequestSeq);
	});

	/**
	 * Queue a commit. It takes its sequence number here, when it is issued, so
	 * a read that is still in flight is recognized as superseded before this
	 * commit has even been answered: that read's body predates the commit, and
	 * filling an emptied field from it would restore the value the commit
	 * removed (the commit's own answer carries the newer state, but it skips
	 * that fill, now that the field is no longer empty). The answer counter is
	 * incremented when the commit settles, rejected or not - a rejection must
	 * consume an answer just as it consumed an issue number - and it is a
	 * `then` on the committer's promise: the apply callback has run by then, and
	 * a queued successor's request starts after it, so the count still tells the
	 * last issued commit apart from an earlier one. It resolves with this
	 * commit's own outcome, because the shared state is already back to
	 * `saving` by the time a callback on this promise runs behind a successor.
	 */
	function commit(patch: SettingsPatch): Promise<CommitStatus> {
		snapshotRequestSeq++;
		commitIssueSeq++;
		modelEmptyAtCommit = !model;
		baseUrlEmptyAtCommit = !customBaseUrl;
		return committer.commit(patch).then((status) => {
			commitAnswerSeq++;
			// Hand the commit's own outcome to the caller: the shared state is
			// already `saving` again by the time this callback runs whenever a
			// successor was queued behind this commit, so re-reading it there
			// would report a rejection for a write that was stored.
			return status;
		});
	}

	/**
	 * Resolves once every settings commit this picker issued has been answered.
	 * A caller that sends an AI request on the same click that completed the
	 * configuration awaits this first, so the request runs against the values
	 * that click stored instead of racing the write (a request that beats it is
	 * answered `llm_not_configured`). Public instance API: reachable through
	 * `bind:this`, and optional at the call site because a caller may unmount
	 * the picker while its settings block is collapsed.
	 */
	export function settle(): Promise<void> {
		return committer.settle();
	}

	// Hand the caller the same wait, so it survives this instance: the import
	// dialog's picker is unmounted on a tab switch, and a commit issued just
	// before it must stay awaitable when no live instance can answer for it.
	$effect(() => {
		onsettle?.(() => committer.settle());
	});

	async function loadModels() {
		const seq = ++modelsRequestSeq;
		if (!provider) {
			llmModelsLoading = false;
			llmModelsError = null;
			llmModelsErrorDetail = null;
			return;
		}
		if (provider === 'custom' && !customBaseUrl.trim()) {
			llmModels = [];
			llmModelsLoading = false;
			llmModelsError = null;
			llmModelsErrorDetail = null;
			return;
		}
		llmModelsLoading = true;
		llmModelsError = null;
		llmModelsErrorDetail = null;
		try {
			const resp = await listLlmModels(provider);
			if (seq !== modelsRequestSeq) return;
			llmModels = resp.models;
			if (model && !resp.models.includes(model)) {
				llmModelsError = t('llmModelsLoadError');
			}
		} catch (err) {
			if (seq !== modelsRequestSeq) return;
			llmModels = [];
			llmModelsError = t('llmModelsLoadError');
			// A transport failure carries no cause worth reading; anything the
			// provider answered is kept, one disclosure away.
			llmModelsErrorDetail =
				err instanceof ApiError && err.code !== 'REQUEST_FAILED' ? err.message : null;
		} finally {
			if (seq === modelsRequestSeq) {
				llmModelsLoading = false;
			}
		}
	}

	/**
	 * Store the current provider selection. `retry` re-sends a selection whose
	 * commit was rejected, so it keeps the dependent fields exactly as shown:
	 * the switch that failed already emptied them, and only a key typed while
	 * the failure was on screen could still hold something to keep.
	 */
	function commitProviderSelection(retry = false) {
		// Invalidate any in-flight model-list request: a stale response must
		// not repopulate the model select after a provider switch.
		const seq = ++modelsRequestSeq;
		if (!retry) {
			// Only a selection that really moved off the stored provider invalidates
			// its dependent values. Re-selecting the stored provider (a user on a
			// slow link, before the read has answered) must keep them: deleting the
			// key here would leave the install with a provider and no credentials.
			const switched = providerChanged(provider, appliedProvider);
			if (switched) {
				model = '';
				customBaseUrl = '';
				apiKeyInput = '';
				llmModels = [];
			}
		}
		llmModelsError = null;
		providerName = llmProviders.find((p) => p.id === provider)?.name ?? provider;
		// Mark the provider as listed before the commit so the remount effect
		// below does not start a second listing with the pre-commit
		// configuration; the commit's answer triggers the one listing.
		modelsLoadedFor = provider;
		// A provider switch clears the previous provider's model, endpoint and
		// key in the same commit; a selection that changed nothing sends the
		// provider alone.
		commit({ ai: providerChangePatch(provider, appliedProvider) }).then((status) => {
			// A rejected commit stored nothing: listing now would query the
			// stored provider while the select shows the rejected one, and a
			// model chosen from that list would be committed for the stored
			// provider instead.
			if (!commitStored(status)) {
				// This rejection is the invalidation above with no successor: the
				// listing it superseded no longer owns the flag, and nothing else
				// clears it, so the model control would stay a disabled
				// "Loading models..." until the selection is committed again.
				if (seq === modelsRequestSeq) llmModelsLoading = false;
				return;
			}
			// List after the commit: a selection that changed nothing lets the
			// server return the stored endpoint, so even the custom provider
			// has something to list once its answer has been applied.
			if (provider) loadModels();
		});
	}

	function onModelChange() {
		commit({ ai: { model: model.trim() ? model : null } });
	}

	function onBaseUrlChange() {
		if (!customBaseUrl.trim()) {
			// The endpoint is gone: its models and the model chosen from them go
			// with it, and a listing still in flight must not repopulate either.
			modelsRequestSeq++;
			model = '';
			llmModels = [];
			llmModelsLoading = false;
			llmModelsError = null;
			commit({ ai: { customBaseUrl: null, model: null } });
			return;
		}
		commit({ ai: { customBaseUrl } }).then((status) => {
			// Nothing is stored unless the commit succeeded: listing now would
			// query the stored endpoint while the field shows the rejected URL,
			// and a model chosen from that list would be committed for the
			// stored endpoint instead.
			if (!commitStored(status)) return;
			if (provider === 'custom') loadModels();
		});
	}

	function onApiKeyChange() {
		const value = apiKeyInput;
		if (!value.trim()) return;
		commit({ ai: { apiKey: value } }).then((status) => {
			// Nothing is stored unless the commit succeeded: a rejected key must
			// stay in the field so the user can correct and resend it, and no
			// model listing may run against a key the server does not have.
			if (!commitStored(status)) return;
			// The value is stored now; never keep it in the DOM. A value typed
			// while the request was in flight stays untouched.
			if (apiKeyInput === value) apiKeyInput = '';
			// The stored key turns a failed model listing into a successful one,
			// so retry it whenever the provider has a usable endpoint.
			if (provider && (provider !== 'custom' || customBaseUrl.trim())) loadModels();
		});
	}

	function onClearApiKey() {
		apiKeyInput = '';
		commit({ ai: { apiKey: null } });
	}

	function onEnter(event: KeyboardEvent, commitField: () => void) {
		if (event.key !== 'Enter') return;
		event.preventDefault();
		commitField();
	}

	// Re-run the providers load after a failure: resetting llmProvidersLoaded
	// re-triggers the load effect below (the catch keeps the loop from
	// retrying on its own, so this is strictly user-initiated).
	function retryLoadProviders() {
		llmProvidersLoaded = false;
	}

	// Load providers once, then reconcile the stored provider.
	$effect(() => {
		if (!llmProvidersLoaded && !llmProvidersLoading) {
			llmProvidersLoading = true;
			providersReady = true;
			listLlmProviders()
				.then((p) => {
					llmProviders = p;
					llmProvidersLoaded = true;
					llmProvidersLoading = false;
					providersReady = p.length > 0;
					providerName = p.find((pp) => pp.id === provider)?.name ?? provider;
					if (provider && !p.some((pp) => pp.id === provider)) {
						provider = '';
						model = '';
					}
				})
				.catch(() => {
					llmProvidersLoaded = true;
					llmProvidersLoading = false;
					providersReady = false;
				});
		}
	});

	// Read the stored configuration; the caller keeps its state unknown until
	// this succeeds, and a failure is surfaced with a retry.
	function loadStoredSettings() {
		// The read owns the flags that describe what is known. A remount starts
		// with an empty `apiKeyState`, so until this mount's own answer applies
		// the caller's `loaded` from an earlier mount would let the chip claim
		// "Not set" (and hide Clear) for a key the server does have. Reset them
		// so nothing speaks about the configuration before a snapshot of this
		// mount has been applied.
		settingsLoadFailed = false;
		loaded = false;
		configured = false;
		settingsReadSettled = false;
		const seq = ++snapshotRequestSeq;
		getSettings()
			.then((snapshot) => {
				// A commit was answered while this read was in flight: that answer
				// carries the newer state, so the read is dropped whole.
				if (seq <= appliedSnapshotSeq) return;
				appliedSnapshotSeq = seq;
				appliedProvider = snapshot.ai.provider;
				// A field the user already filled is theirs to keep; the read only
				// fills what is still empty, and only while it is the newest
				// answer: a commit issued after this read started holds a higher
				// number and its body predates that commit, so filling from it
				// would show the model and the endpoint the commit removed. The
				// model and the endpoint are also only filled while the local
				// provider is the one the snapshot reports: a selection made
				// before this answer moved off that provider, and the server
				// deleted the model and the endpoint with it, so filling them
				// here would show a model the server no longer has.
				const fills = isNewest(seq);
				if (fills && !provider) provider = snapshot.ai.provider;
				if (provider === snapshot.ai.provider) {
					if (fills && !model) model = snapshot.ai.model;
					if (fills && !customBaseUrl) customBaseUrl = snapshot.ai.customBaseUrl;
				}
				apiKeyState = snapshot.ai.apiKey;
				configured = isAiConfigured(snapshot);
				loaded = true;
				providerName = llmProviders.find((p) => p.id === provider)?.name ?? provider;
			})
			.catch(() => {
				// The read failed, so the stored configuration stays unknown:
				// `loaded` remains false instead of reporting the install as
				// unconfigured, and the retry below is the only way back.
				if (seq <= appliedSnapshotSeq) return;
				settingsLoadFailed = true;
			})
			.finally(() => {
				// The listing effect below waits for this: while the read is in
				// flight its answer may still supply the stored provider and the
				// stored endpoint, so listing now would only be repeated.
				settingsReadSettled = true;
			});
	}

	// Re-run the read after a failure: clearing `restored` re-triggers the
	// effect below (the catch above never retries on its own).
	function retryLoadSettings() {
		restored = false;
	}

	// Load the stored configuration once per mount; never overwrite user edits.
	$effect(() => {
		if (restored) return;
		restored = true;
		loadStoredSettings();
	});

	// Reload models when the picker remounts with a provider already selected.
	// Gated on the mount read having settled: that read may still supply the
	// stored provider (or the stored custom endpoint the listing needs), so
	// listing before its answer would run twice for the same state change.
	// Gated on the same predicate as the commit path: a shown provider whose
	// commit was rejected is not the stored one, so listing it would query the
	// environment's credentials while the server still holds another provider -
	// and a model picked from that list would be committed for the provider the
	// server has, not the one on screen. The `Retry` below stores the rejected
	// selection again and lists it then. The provider must also be confirmed by
	// a snapshot (`appliedProvider !== null`): after a failed settings read
	// nothing names the stored provider, and listing the bound one would
	// contact the environment for a provider the server may not hold - the
	// read's retry lists once an answer names the provider.
	$effect(() => {
		if (!settingsReadSettled) return;
		if (
			provider &&
			appliedProvider !== null &&
			!providerChanged(provider, appliedProvider) &&
			modelsLoadedFor !== provider
		) {
			modelsLoadedFor = provider;
			loadModels();
		}
	});
</script>

{#if llmProviders.length === 0 && !llmProvidersLoading}
	<p class="form-error">{t('llmNoProviders')}</p>
	{#if llmProvidersLoaded}
		<button type="button" class="btn btn--ghost" onclick={retryLoadProviders} disabled={disabled}>
			{t('buttonRetry')}
		</button>
	{/if}
{:else}
	<div class="llm-fields" class:import-subsection={variant === 'panel'}>
		<div class="field">
			<div class="field__head">
				<label class="field__label" for="llm-provider">{t('llmProviderLabel')}</label>
			</div>
			<!-- Enter retries the selection: a rejected commit leaves a provider
			     the server does not have, and a select cannot re-fire `change`
			     for the value it already shows. -->
			<select id="llm-provider" bind:value={provider} onchange={() => commitProviderSelection()}
				onkeydown={(e) => onEnter(e, () => commitProviderSelection(true))}
				disabled={llmProvidersLoading || disabled}>
				<option value="">{t('llmProviderPlaceholder')}</option>
				{#each llmProviders as p}
					<!-- Never disable a provider: `configured` only reports whether a
					     key is available right now, and a disabled option would make
					     storing that key from this picker impossible. -->
					<option value={p.id}>
						{p.name}{p.configured ? '' : ` (${t('notConfigured')})`}
					</option>
				{/each}
			</select>
		</div>

		{#if provider}
			<div class="field">
				<div class="field__head">
					<label class="field__label" for="llm-model">{t('llmModelLabel')}</label>
					{#if llmModelsLoading}
						<span class="state-chip state-chip--neutral">
							<span class="state-chip__icon" aria-hidden="true"><Icon name="loader-circle" size={12} spin /></span>
							{t('llmModelLoading')}
						</span>
					{/if}
				</div>
				{#if llmModelsError}
					<!-- The listing failed, so the model is entered by hand. -->
					<input id="llm-model" type="text" bind:value={model} placeholder={t('importLlmModelPlaceholder')}
						disabled={disabled} onchange={onModelChange}
						onkeydown={(e) => onEnter(e, onModelChange)} />
				{:else}
					<select id="llm-model" bind:value={model} disabled={disabled || llmModelsLoading}
						onchange={onModelChange} onkeydown={(e) => onEnter(e, onModelChange)}>
						<option value="">{llmModelsLoading ? t('llmModelLoading') : t('llmModelPlaceholder')}</option>
						{#each llmModels as m}
							<option value={m}>{m}</option>
						{/each}
					</select>
				{/if}
			</div>
		{/if}

		{#if provider === 'custom'}
			<p class="llm-hint">{t('llmCustomHint')}</p>
			<div class="field">
				<div class="field__head">
					<label class="field__label" for="llm-base-url">{t('llmCustomBaseUrlLabel')}</label>
				</div>
				<input id="llm-base-url" type="url" bind:value={customBaseUrl} placeholder={t('llmCustomBaseUrlPlaceholder')}
					disabled={disabled} onchange={onBaseUrlChange}
					onkeydown={(e) => onEnter(e, onBaseUrlChange)} />
			</div>
		{/if}

		{#if provider}
			<div class="field">
				<div class="field__head">
					<label class="field__label" for="llm-api-key">
						{provider === 'custom' ? t('llmCustomApiKeyLabel') : t('settingsApiKeyLabel')}
					</label>
					<!-- Only an applied snapshot knows where the key comes from:
					     before one (and after a read that failed) the chip would
					     claim "Not set" for a key that may well be stored. -->
					{#if loaded}
						<span class="state-chip state-chip--neutral llm-secret-state">{t(secretSourceLabelKey(apiKeyState.source))}</span>
						{#if apiKeyState.source === 'settings'}
							<button type="button" class="btn btn--ghost btn--compact" onclick={onClearApiKey} disabled={disabled}>
								{t('settingsSecretClear')}
							</button>
						{/if}
					{/if}
				</div>
				<input id="llm-api-key" type="password" bind:value={apiKeyInput}
					placeholder={provider === 'custom' ? t('llmCustomApiKeyPlaceholder') : t('settingsApiKeyPlaceholder')}
					disabled={disabled} onchange={onApiKeyChange}
					onkeydown={(e) => onEnter(e, onApiKeyChange)} />
			</div>
		{/if}

		{#if llmModelsError}
			<div class="llm-error">
				<p class="form-error" role="alert">{llmModelsError}</p>
				{#if llmModelsErrorDetail}
					<details class="llm-error__details">
						<summary>{t('llmErrorDetails')}</summary>
						<p>{llmModelsErrorDetail}</p>
					</details>
				{/if}
			</div>
		{/if}
		{#if provider === 'ollama' && llmModelsError}
			<p class="llm-hint">{t('llmOllamaHint')}</p>
		{/if}

		{#if commitStatusLabelKey(committer.state.status)}
			<p class="llm-commit-state" role="status">
				<span class="state-chip {commitStatusChipClass(committer.state.status)}">
					{t(commitStatusLabelKey(committer.state.status)!)}
				</span>
			</p>
		{/if}
		{#if committer.state.status === 'error'}
			<p class="form-error" role="alert">{committer.state.error}</p>
			{#if provider && providerChanged(provider, appliedProvider)}
				<!-- The shown provider is the one whose commit was rejected, so it
				     is not stored and its models are not listed: resending the
				     selection is the way back to a usable state. -->
				<button type="button" class="btn btn--ghost" onclick={() => commitProviderSelection(true)}
					disabled={disabled}>
					{t('buttonRetry')}
				</button>
			{/if}
		{/if}
	</div>
{/if}

{#if settingsLoadFailed}
	<div class="llm-settings-error">
		<p class="form-error" role="alert">{t('settingsLoadFailed')}</p>
		<button type="button" class="btn btn--ghost" onclick={retryLoadSettings} disabled={disabled}>
			{t('buttonRetry')}
		</button>
	</div>
{/if}

<style>
	.llm-fields {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	/* Panel chrome for the import dialogs; the settings page brings its own card. */
	.import-subsection {
		padding: var(--space-3) var(--space-4);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
	}

	.llm-hint {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
		line-height: 1.5;
	}

	.llm-error {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	/* The stored settings could not be read: the picker stays empty, so the
	   failure and its retry sit outside the field stack. */
	.llm-settings-error {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		align-items: flex-start;
	}
	.llm-settings-error .form-error {
		margin: 0;
	}
	.llm-error .form-error {
		margin: 0;
		align-items: flex-start;
	}
	.llm-error__details {
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
	.llm-error__details summary {
		cursor: pointer;
		color: var(--color-text-secondary);
	}
	.llm-error__details p {
		margin: var(--space-2) 0 0;
		padding: var(--space-2) var(--space-3);
		background: var(--color-surface-2);
		border-radius: var(--radius-sm);
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		font-size: var(--text-xs);
		overflow-wrap: anywhere;
	}

	/* Test hook: the E2E suite pins the provenance of the stored AI key here. */
	.llm-secret-state {
		margin: 0;
	}
	.llm-commit-state {
		margin: 0;
	}
</style>
