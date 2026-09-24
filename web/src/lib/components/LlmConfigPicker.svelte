<script lang="ts">
	import { listLlmProviders, listLlmModels, getSettings, updateSettings, ApiError } from '$lib/api';
	import {
		SettingsCommitter,
		commitStatusChipClass,
		isAiConfigured,
		commitStatusLabelKey,
		secretSourceLabelKey,
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
	// Provider whose models were already loaded this mount; ensures a fresh
	// model load after the component remounts (collapse/expand, tab switch).
	let modelsLoadedFor: string | null = null;
	// Monotonic sequence for model-list requests: a slow earlier response must
	// not overwrite the models of a newer provider switch.
	let modelsRequestSeq = 0;
	// The same discipline for settings snapshots: the mount-time read takes its
	// number before the request starts, so an answer that lands after a newer
	// commit answer is discarded instead of undoing that commit.
	let appliedSnapshotSeq = 0;
	let snapshotRequestSeq = 0;

	/** Server-owned snapshot fields, applied from the newest answer only. */
	function applySnapshot(snapshot: SettingsSnapshot, seq: number) {
		if (seq <= appliedSnapshotSeq) return;
		appliedSnapshotSeq = seq;
		apiKeyState = snapshot.ai.apiKey;
		configured = isAiConfigured(snapshot);
		loaded = true;
	}

	// Commits are serialized: a settings change may only be followed by the
	// next one once the previous request has been answered, so a slow
	// provider commit can never land after a newer model commit.
	const committer = new SettingsCommitter(updateSettings, (snapshot) =>
		applySnapshot(snapshot, ++snapshotRequestSeq),
	);

	function commit(patch: SettingsPatch): Promise<void> {
		return committer.commit(patch);
	}

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

	function onProviderChange() {
		// Invalidate any in-flight model-list request: a stale response must
		// not repopulate the model select after a provider switch.
		modelsRequestSeq++;
		model = '';
		customBaseUrl = '';
		apiKeyInput = '';
		llmModels = [];
		llmModelsError = null;
		providerName = llmProviders.find((p) => p.id === provider)?.name ?? provider;
		// A provider switch invalidates the previous provider's model,
		// endpoint and key: clear them in the same commit.
		commit({
			ai: { provider, model: null, customBaseUrl: null, apiKey: null },
		}).then(() => {
			modelsLoadedFor = provider;
			if (provider && provider !== 'custom') loadModels();
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
		commit({ ai: { customBaseUrl } }).then(() => {
			if (provider === 'custom') loadModels();
		});
	}

	function onApiKeyChange() {
		const value = apiKeyInput;
		if (!value.trim()) return;
		commit({ ai: { apiKey: value } }).then(() => {
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

	// Load the stored configuration once per mount; never overwrite user edits.
	$effect(() => {
		if (restored) return;
		restored = true;
		const seq = ++snapshotRequestSeq;
		getSettings()
			.then((snapshot) => {
				// A commit was answered while this read was in flight: that answer
				// carries the newer state, so the read is dropped whole.
				if (seq <= appliedSnapshotSeq) return;
				appliedSnapshotSeq = seq;
				// A field the user already filled is theirs to keep; the read only
				// fills what is still empty.
				if (!provider) provider = snapshot.ai.provider;
				if (!model) model = snapshot.ai.model;
				if (!customBaseUrl) customBaseUrl = snapshot.ai.customBaseUrl;
				apiKeyState = snapshot.ai.apiKey;
				configured = isAiConfigured(snapshot);
				loaded = true;
				providerName = llmProviders.find((p) => p.id === provider)?.name ?? provider;
				// List the restored provider's models straight away, so a stored
				// model is displayed and stays editable; `loadModels` returns
				// early while a custom provider has no stored base URL yet.
				if (provider) {
					modelsLoadedFor = provider;
					loadModels();
				}
			})
			.catch(() => {
				// A settings read failure leaves the picker empty; the settings
				// page surfaces the error again on its own load.
			});
	});

	// Reload models when the picker remounts with a provider already selected.
	$effect(() => {
		if (provider && modelsLoadedFor !== provider) {
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
			<select id="llm-provider" bind:value={provider} onchange={onProviderChange}
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
						<span class="state-chip state-chip--none">
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
						onchange={onModelChange}>
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
					<span class="state-chip state-chip--neutral llm-secret-state">{t(secretSourceLabelKey(apiKeyState.source))}</span>
					{#if apiKeyState.source === 'settings'}
						<button type="button" class="btn btn--ghost btn--compact" onclick={onClearApiKey} disabled={disabled}>
							{t('settingsSecretClear')}
						</button>
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
		{/if}
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
