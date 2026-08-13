<script lang="ts">
	import { listLlmProviders, listLlmModels, ApiError } from '$lib/api';
	import { readStoredLlmConfig } from '$lib/llm-config.svelte';
	import { t } from '$lib/i18n';
	import type { LlmProviderInfo } from '$lib/types';

	let {
		provider = $bindable(''),
		providerName = $bindable(''),
		model = $bindable(''),
		customBaseUrl = $bindable(''),
		customApiKey = $bindable(''),
		disabled = false,
		providersReady = $bindable(true),
		autorestore = true,
		onrestored,
	}: {
		provider?: string;
		providerName?: string;
		model?: string;
		customBaseUrl?: string;
		customApiKey?: string;
		disabled?: boolean;
		providersReady?: boolean;
		autorestore?: boolean;
		onrestored?: () => void;
	} = $props();

	let llmProviders = $state<LlmProviderInfo[]>([]);
	let llmProvidersLoading = $state(false);
	let llmProvidersLoaded = $state(false);
	let llmModels: string[] = $state([]);
	let llmModelsLoading = $state(false);
	let llmModelsError = $state<string | null>(null);
	let restored = $state(false);
	// Provider whose models were already loaded this mount; ensures a fresh
	// model load after the component remounts (collapse/expand, tab switch).
	let modelsLoadedFor: string | null = null;
	// Monotonic sequence for model-list requests: a slow earlier response must
	// not overwrite the models of a newer provider switch.
	let modelsRequestSeq = 0;

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
			const resp = await listLlmModels(
				provider,
				provider === 'custom' ? customBaseUrl : undefined,
				provider === 'custom' ? customApiKey || undefined : undefined,
			);
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
		// not repopulate the model select after a provider switch (incl. to
		// 'custom', which does not trigger a new loadModels()).
		modelsRequestSeq++;
		model = '';
		llmModels = [];
		llmModelsError = null;
		customBaseUrl = '';
		customApiKey = '';
		if (provider && provider !== 'custom') {
			loadModels();
		}
		// onProviderChange already (re)loads models directly; mark the new
		// provider so the remount-reload effect below does not load twice.
		modelsLoadedFor = provider;
	}

	// Re-run the providers load after a failure: resetting llmProvidersLoaded
	// re-triggers the load effect below (the catch keeps the loop from
	// retrying on its own, so this is strictly user-initiated).
	function retryLoadProviders() {
		llmProvidersLoaded = false;
	}

	// Publish the current provider's display name (once providers load and on
	// every provider change) so collapsed summaries can show it instead of the id.
	$effect(() => {
		providerName = llmProviders.find((p) => p.id === provider)?.name ?? providerName;
	});

	$effect(() => {
		// Restore stored config once per mount; never overwrite user edits.
		if (autorestore && !restored) {
			restored = true;
			const stored = readStoredLlmConfig();
			if (stored && !provider) {
				provider = stored.provider;
				model = stored.model;
				customBaseUrl = stored.customBaseUrl;
				customApiKey = stored.customApiKey;
				// The direct loadModels() below covers the restored provider.
				modelsLoadedFor = stored.provider;
				if (stored.provider && stored.provider !== 'custom') {
					loadModels();
				}
				if (stored.provider && stored.model) {
					onrestored?.();
				}
			}
		}
		// Load providers once, then reconcile the restored provider.
		if (!llmProvidersLoaded && !llmProvidersLoading) {
			llmProvidersLoading = true;
			providersReady = true;
			listLlmProviders()
				.then((p) => {
					llmProviders = p;
					llmProvidersLoaded = true;
					llmProvidersLoading = false;
					providersReady = p.length > 0;
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

	// Debounced model loading for custom endpoint URL / API key changes.
	let _customDebounceTimer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		customBaseUrl;
		customApiKey;
		if (provider === 'custom' && customBaseUrl.trim()) {
			if (_customDebounceTimer) clearTimeout(_customDebounceTimer);
			_customDebounceTimer = setTimeout(() => {
				loadModels();
			}, 500);
		} else if (provider === 'custom') {
			// Base URL cleared: drop the stale model list instead of leaving
			// the previously loaded models selectable. Bump the seq first so a
			// listLlmModels response still in flight is invalidated and cannot
			// repopulate the list (or surface a stale error) afterwards.
			modelsRequestSeq++;
			model = '';
			llmModels = [];
			llmModelsLoading = false;
			llmModelsError = null;
		}
		return () => {
			if (_customDebounceTimer) clearTimeout(_customDebounceTimer);
		};
	});

	// Reload models when the picker remounts with a provider already selected.
	// Collapse/expand toggles and tab switches unmount this component, which
	// resets the per-mount llmModels list, so nothing else would repopulate it.
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
						disabled={disabled} />
				{:else}
					<select bind:value={model} aria-label={t('llmModelLabel')} disabled={disabled}>
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
					disabled={disabled} />
			</label>
			<label class="import-field">
				<span>{t('llmCustomApiKeyLabel')}</span>
				<input type="password" bind:value={customApiKey} placeholder={t('llmCustomApiKeyPlaceholder')}
					disabled={disabled} />
			</label>
		{/if}

		{#if llmModelsError}
			<p class="form-error">{llmModelsError}</p>
		{/if}
		{#if provider === 'ollama' && llmModelsError}
			<p class="import-info">{t('llmOllamaHint')}</p>
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
</style>
