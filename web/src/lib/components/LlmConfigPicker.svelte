<script lang="ts">
	import { listLlmProviders, listLlmModels, getSettings, updateSettings, ApiError } from '$lib/api';
	import {
		SettingsCommitter,
		isAiConfigured,
		commitStatusLabelKey,
		secretSourceLabelKey,
		type SecretState,
		type SettingsPatch,
	} from '$lib/settings.svelte';
	import { t } from '$lib/i18n';
	import type { LlmProviderInfo } from '$lib/types';

	let {
		provider = $bindable(''),
		providerName = $bindable(''),
		model = $bindable(''),
		disabled = false,
		providersReady = $bindable(true),
		configured = $bindable(false),
	}: {
		provider?: string;
		providerName?: string;
		model?: string;
		disabled?: boolean;
		providersReady?: boolean;
		configured?: boolean;
	} = $props();

	let llmProviders = $state<LlmProviderInfo[]>([]);
	let llmProvidersLoading = $state(false);
	let llmProvidersLoaded = $state(false);
	let llmModels: string[] = $state([]);
	let llmModelsLoading = $state(false);
	let llmModelsError = $state<string | null>(null);
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

	// Commits are serialized: a settings change may only be followed by the
	// next one once the previous request has been answered, so a slow
	// provider commit can never land after a newer model commit.
	const committer = new SettingsCommitter(updateSettings, (snapshot) => {
		apiKeyState = snapshot.ai.apiKey;
		configured = isAiConfigured(snapshot);
	});

	function commit(patch: SettingsPatch): Promise<void> {
		return committer.commit(patch);
	}

	async function loadModels() {
		const seq = ++modelsRequestSeq;
		if (!provider) {
			llmModelsLoading = false;
			llmModelsError = null;
			return;
		}
		if (provider === 'custom' && !customBaseUrl.trim()) {
			llmModels = [];
			llmModelsLoading = false;
			llmModelsError = null;
			return;
		}
		llmModelsLoading = true;
		llmModelsError = null;
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
			llmModelsError = err instanceof ApiError
				? (err.code === 'REQUEST_FAILED' ? t('llmModelsLoadError') : `${t('llmModelsLoadError')} (${err.message})`)
				: t('llmModelsLoadError');
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
		getSettings()
			.then((snapshot) => {
				provider = snapshot.ai.provider;
				model = snapshot.ai.model;
				customBaseUrl = snapshot.ai.customBaseUrl;
				apiKeyState = snapshot.ai.apiKey;
				configured = isAiConfigured(snapshot);
				providerName = llmProviders.find((p) => p.id === provider)?.name ?? provider;
				// Standard providers list their models straight away; the custom
				// provider waits for a base URL to be stored.
				if (provider && provider !== 'custom') {
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
		if (provider && provider !== 'custom' && modelsLoadedFor !== provider) {
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
	<div class="import-subsection">
		<div class="llm-provider-row">
			<select bind:value={provider} onchange={onProviderChange}
				aria-label={t('llmProviderLabel')}
				disabled={llmProvidersLoading || disabled}>
				<option value="">{t('llmProviderPlaceholder')}</option>
				{#each llmProviders as p}
					<option value={p.id} disabled={!p.configured && p.id !== 'ollama'}>
						{p.name}{p.configured ? '' : ` (${t('notConfigured')})`}
					</option>
				{/each}
			</select>

			{#if provider}
				{#if llmModelsLoading}
					<span class="import-loading">{t('llmModelLoading')}</span>
				{:else if llmModelsError}
					<input type="text" bind:value={model} placeholder={t('importLlmModelPlaceholder')}
						disabled={disabled} onchange={onModelChange}
						onkeydown={(e) => onEnter(e, onModelChange)} />
				{:else}
					<select bind:value={model} aria-label={t('llmModelLabel')} disabled={disabled}
						onchange={onModelChange}>
						<option value="">{t('llmModelPlaceholder')}</option>
						{#each llmModels as m}
							<option value={m}>{m}</option>
						{/each}
					</select>
				{/if}
			{/if}
		</div>

		{#if provider === 'custom'}
			<p class="import-info">{t('llmCustomHint')}</p>
			<label class="import-field">
				<span>{t('llmCustomBaseUrlLabel')}</span>
				<input type="url" bind:value={customBaseUrl} placeholder={t('llmCustomBaseUrlPlaceholder')}
					disabled={disabled} onchange={onBaseUrlChange}
					onkeydown={(e) => onEnter(e, onBaseUrlChange)} />
			</label>
		{/if}

		{#if provider}
			<label class="import-field">
				<span>{provider === 'custom' ? t('llmCustomApiKeyLabel') : t('settingsApiKeyLabel')}</span>
				<input type="password" bind:value={apiKeyInput}
					placeholder={provider === 'custom' ? t('llmCustomApiKeyPlaceholder') : t('settingsApiKeyPlaceholder')}
					disabled={disabled} onchange={onApiKeyChange}
					onkeydown={(e) => onEnter(e, onApiKeyChange)} />
			</label>
			<p class="llm-secret-state">
				{t(secretSourceLabelKey(apiKeyState.source))}
				{#if apiKeyState.source === 'settings'}
					<button type="button" class="btn btn--ghost" onclick={onClearApiKey} disabled={disabled}>
						{t('settingsSecretClear')}
					</button>
				{/if}
			</p>
		{/if}

		{#if llmModelsError}
			<p class="form-error">{llmModelsError}</p>
		{/if}
		{#if provider === 'ollama' && llmModelsError}
			<p class="import-info">{t('llmOllamaHint')}</p>
		{/if}

		<p class="llm-commit-state" role="status">
			{commitStatusLabelKey(committer.state.status) ? t(commitStatusLabelKey(committer.state.status)!) : ''}
		</p>
		{#if committer.state.status === 'error'}
			<p class="form-error" role="alert">{committer.state.error}</p>
		{/if}
	</div>
{/if}

<style>
	.import-subsection {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
	}

	.llm-provider-row {
		display: flex;
		gap: var(--space-2);
		align-items: flex-start;
	}
	.llm-provider-row > * {
		flex: 1;
		min-width: 0;
	}

	.llm-secret-state {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
	.llm-commit-state {
		min-height: 1.2em;
		font-size: var(--text-sm);
		color: var(--color-text-muted);
	}
</style>
