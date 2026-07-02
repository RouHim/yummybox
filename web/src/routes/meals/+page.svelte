<script lang="ts">
    import { listMeals, updateMeal, deleteMeal, mealImageUrl, createMeal, importFromUrl, importFromPaste, importFromLlm, importBulk, listLlmProviders, listLlmModels, ApiError, loadImageFromUrl } from '$lib/api';
	import type { Meal, NewIngredientLine } from '$lib/types';
import { readStoredLlmConfig, persistLlmConfig } from '$lib/llm-config.svelte';
	import { t, formatDate } from '$lib/i18n';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';

	import { fly, fade, scale } from 'svelte/transition';
	import { flip } from 'svelte/animate';
	import { prefersReducedMotion, tierDuration, staggerDuration } from '$lib/motion';
	import Icon from '$lib/Icon.svelte';
	import DeleteConfirmDialog from '$lib/DeleteConfirmDialog.svelte';
	import MealForm from '$lib/MealForm.svelte';
	let meals = $state<Meal[]>([]);
	let searchTerm = $state('');
	let loadError = $state<string | null>(null);
	let reduced = $state(prefersReducedMotion());
	let deleteTarget = $state<Meal | null>(null);
	let editHandled = false;

	let editTarget = $state<Meal | null>(null);
	let editSubmitting = $state(false);

	let addOpen = $state(false);
	let formName = $state('');
	let formIngredients = $state<NewIngredientLine[]>([{ name: '', quantity: null }]);
	let formInstructions = $state('');
	let formImage = $state<File | null>(null);
	let removeImage = $state(false);
	let submitting = $state(false);
	let importMode = $state<'link' | 'llm' | 'bulk'>('link');
	let importCollapsed = $state(false);
	let importInput = $state('');
    let importLlmProvider = $state('');
    let importLlmModel = $state('');
    let importLlmHint = $state('');
    let importing = $state(false);
    let importError = $state<string | null>(null);
    let importToken = $state(0);
    let llmProviders = $state<import('$lib/types').LlmProviderInfo[]>([]);
    let llmProvidersLoading = $state(false);
    let llmProvidersLoaded = $state(false);
    let llmModels: string[] = $state([]);
    let llmModelsLoading = $state(false);
    let llmModelsError = $state<string | null>(null);
    let importLlmCustomBaseUrl = $state('');
    let llmConfigRestored = false;
    let importLlmCustomApiKey = $state('');
    let llmSettingsCollapsed = $state(false);

    let bulkUrls = $state('');
    let bulkImporting = $state(false);
    let bulkResult = $state<import('$lib/types').BulkImportResult | null>(null);
    let bulkError = $state<string | null>(null);
    async function onImport() {
        importError = null;
        importing = true;
        try {
            const input = importInput.trim();
            const draft = importMode === 'link'
                ? (input.startsWith('http://') || input.startsWith('https://')
                    ? await importFromUrl(input)
                    : await importFromPaste(importInput))
                : await importFromLlm(
                    importLlmModel, importLlmHint || null, null,
                    importLlmProvider === 'custom' ? importLlmCustomBaseUrl : undefined,
                    importLlmProvider === 'custom' ? importLlmCustomApiKey : undefined,
                );
            formName = draft.name;
            formIngredients = draft.ingredients.length > 0
                ? draft.ingredients.map(i => ({ name: i.name, quantity: i.quantity }))
                : [{ name: '', quantity: null }];
            formInstructions = draft.instructions;
            if (draft.imageBase64) {
                const bytes = Uint8Array.from(atob(draft.imageBase64), c => c.charCodeAt(0));
                formImage = new File([bytes], 'imported.jpg', { type: 'image/jpeg' });
                removeImage = false;
            }
            if (importMode === 'llm') {
                persistLlmConfig({
                    provider: importLlmProvider,
                    model: importLlmModel,
                    customBaseUrl: importLlmCustomBaseUrl,
                    customApiKey: importLlmCustomApiKey,
                });
            }
            importInput = '';
            importLlmHint = '';
            importToken++;
            importCollapsed = true;
        } catch (err) {
            if (err instanceof ApiError) {
                if (err.code === 'llm_timeout') {
                    importError = t('llmErrorTimeout');
                } else if (err.code === 'llm_parse_failed') {
                    importError = t('llmErrorParseFailed');
                } else if (err.code) {
                    importError = t('llmErrorGeneric', { message: err.message });
                } else {
                    importError = err.message === '__REQUEST_FAILED__' ? t('importErrorFetch') : err.message;
                }
            } else {
                importError = err instanceof Error ? err.message : '';
            }
        } finally {
            importing = false;
        }
    }


    async function loadLlmModels() {
        if (!importLlmProvider) return;
        if (importLlmProvider === 'custom' && !importLlmCustomBaseUrl.trim()) return;
        llmModelsLoading = true;
        llmModelsError = null;
        try {
            const resp = await listLlmModels(
                importLlmProvider,
                importLlmProvider === 'custom' ? importLlmCustomBaseUrl : undefined,
                importLlmProvider === 'custom' ? importLlmCustomApiKey || undefined : undefined,
            );
            llmModels = resp.models;
            if (importLlmModel && !resp.models.includes(importLlmModel)) {
                llmModelsError = t('llmModelsLoadError');
            }
        } catch (err) {
            llmModels = [];
            if (err instanceof ApiError) {
                llmModelsError = err.message === '__REQUEST_FAILED__'
                    ? t('llmModelsLoadError')
                    : `${t('llmModelsLoadError')} (${err.message})`;
            } else {
                llmModelsError = t('llmModelsLoadError');
            }
        } finally {
            llmModelsLoading = false;
        }
    }

    function onProviderChange() {
        importLlmModel = '';
        llmModels = [];
        llmModelsError = null;
        importLlmCustomBaseUrl = '';
        importLlmCustomApiKey = '';
        if (importLlmProvider && importLlmProvider !== 'custom') {
            loadLlmModels();
        }
    }
    async function onBulkImport() {
        bulkError = null;
        bulkResult = null;
        const lines = bulkUrls.split('\n').map(l => l.trim()).filter(l => l.length > 0);
        if (lines.length === 0) return;
        if (lines.length > 50) {
            bulkError = t('importBulkErrorMaxUrls');
            return;
        }
        bulkImporting = true;
        try {
            const result = await importBulk({ urls: lines });
            if (result.created.length > 0) {
                await loadMeals();
                addOpen = false;
                return;
            }
            bulkResult = result;
        } catch (err) {
            bulkError = err instanceof ApiError
                ? (err.message === '__REQUEST_FAILED__' ? t('importErrorFetch') : err.message)
                : (err instanceof Error ? err.message : '');
        } finally {
            bulkImporting = false;
        }
    }

    // Restore stored LLM config when opening the import card
    $effect(() => {
        if (importMode === 'llm' && !llmConfigRestored) {
            llmConfigRestored = true;
            const stored = readStoredLlmConfig();
            if (stored) {
                importLlmProvider = stored.provider;
                importLlmModel = stored.model;
                importLlmCustomBaseUrl = stored.customBaseUrl;
                importLlmCustomApiKey = stored.customApiKey;
                // Trigger model loading for standard providers;
                // custom providers are picked up by the debounce effect below.
                if (stored.provider && stored.provider !== 'custom') {
                    loadLlmModels();
                }
                if (stored.provider && stored.model) {
                    llmSettingsCollapsed = true;
                }
            }
        }
    });

    // Load providers when LLM tab is first activated
    $effect(() => {
        if (importMode === 'llm' && !llmProvidersLoaded && !llmProvidersLoading) {
            llmProvidersLoading = true;
            listLlmProviders().then(p => {
                llmProviders = p;
                llmProvidersLoaded = true;
                llmProvidersLoading = false;
                // Reconcile restored provider against live list
                if (importLlmProvider && !p.some(pp => pp.id === importLlmProvider)) {
                    importLlmProvider = '';
                    importLlmModel = '';
                }
            }).catch(() => {
                llmProvidersLoading = false;
            });
        }
    });

    // Debounced model loading for custom endpoint URL / API key changes
    let _customDebounceTimer: ReturnType<typeof setTimeout> | undefined;
    $effect(() => {
        // Read both so the effect re-fires on either change
        importLlmCustomBaseUrl;
        importLlmCustomApiKey;
        if (importLlmProvider === 'custom' && importLlmCustomBaseUrl.trim()) {
            if (_customDebounceTimer) clearTimeout(_customDebounceTimer);
            _customDebounceTimer = setTimeout(() => {
                loadLlmModels();
            }, 500);
        }
        return () => {
            if (_customDebounceTimer) clearTimeout(_customDebounceTimer);
        };
    });

	async function onSubmitAdd(payload: {
		name: string; ingredients: NewIngredientLine[]; instructions: string;
		image: File | null; removeImage: boolean;
	}) {
		submitting = true;
		try {
			await createMeal(
				{ name: payload.name, ingredients: payload.ingredients, instructions: payload.instructions },
				payload.image
			);
			await loadMeals();
			closeAdd();
		} finally {
			submitting = false;
		}
	}

    function openAdd() {
        formName = ''; formIngredients = [{ name: '', quantity: null }]; formInstructions = '';
        formImage = null; removeImage = false; submitting = false; llmConfigRestored = false;
        importMode = 'link'; importInput = '';
        importLlmProvider = ''; importLlmModel = ''; importLlmHint = '';
        llmSettingsCollapsed = false;
        importing = false; importError = null; importToken++;
        importCollapsed = false;
        bulkUrls = ''; bulkImporting = false; bulkResult = null; bulkError = null;
        addOpen = true;
        editHandled = false;
    }
	function closeAdd() {
		if (bulkResult && bulkResult.created.length > 0) {
			loadMeals();
		}
		addOpen = false;
	}


	$effect(() => {
		const mq = window.matchMedia('(prefers-reduced-motion: reduce)');
		const handler = (e: MediaQueryListEvent) => (reduced = e.matches);
		mq.addEventListener('change', handler);
		return () => mq.removeEventListener('change', handler);
	});


	async function loadMeals() {
		try {
			meals = await listMeals(searchTerm || undefined);
			loadError = null;
		} catch (err) {
			const raw = err instanceof Error ? err.message : '';
			loadError = raw === '__REQUEST_FAILED__' ? t('errorLoadFailed') : raw;
		}
	}

	$effect(() => {
		loadMeals();
	});

	// Deep-link: open the edit modal for ?edit=<id>
	$effect(() => {
		const raw = page.url.searchParams.get('edit');
		if (!raw || editHandled || meals.length === 0) return;
		const id = Number(raw);
		if (Number.isNaN(id)) return;
		const meal = meals.find(m => m.id === id);
		if (meal) {
			editHandled = true;
			openEdit(meal);
			goto('/meals', { replaceState: true, keepFocus: true, noScroll: true });
		}
	});

	$effect(() => {
		const term = searchTerm;
		const handle = setTimeout(() => {
			loadMeals();
		}, 150);
		return () => clearTimeout(handle);
	});

	function openEdit(meal: Meal) {
		editTarget = meal;
	}

	function closeEdit() {
		editTarget = null;
	}

	async function onSubmitEdit(payload: {
		name: string; ingredients: NewIngredientLine[]; instructions: string;
		image: File | null; removeImage: boolean;
	}) {
		// editTarget is non-null while the modal is open
		const id = editTarget!.id;
		editSubmitting = true;
		try {
			await updateMeal(id, { name: payload.name, ingredients: payload.ingredients, instructions: payload.instructions }, {
				image: payload.image,
				removeImage: payload.removeImage,
			});
			await loadMeals();
			closeEdit();
		} finally {
			editSubmitting = false;
		}
	}


	async function onDelete(meal: Meal) {
		deleteTarget = meal;
	}

	async function confirmDeleteDelete() {
		if (!deleteTarget) return;
		const id = deleteTarget.id;
		deleteTarget = null;
		try {
			await deleteMeal(id);
			await loadMeals();
		} catch (err) {
			const raw = err instanceof Error ? err.message : '';
			loadError = raw === '__REQUEST_FAILED__' ? t('errorDeleteFailed') : raw;
		}
	}

	function ingredientPreview(meal: Meal): string {
		return meal.ingredients.map(i => i.quantity ? `${i.name} (${i.quantity})` : i.name).join(', ');
	}

	// matches DeleteConfirmDialog.svelte focusTrap
	function focusTrap(node: HTMLElement) {
		const previouslyFocused = document.activeElement as HTMLElement | null;
		node.focus();
		function onKey(e: KeyboardEvent) {
			if (e.key !== 'Tab') return;
			const focusables = node.querySelectorAll<HTMLElement>(
				'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
			);
			if (focusables.length === 0) return;
			const first = focusables[0];
			const last = focusables[focusables.length - 1];
			if (e.shiftKey && document.activeElement === first) {
				e.preventDefault();
				last.focus();
			} else if (!e.shiftKey && document.activeElement === last) {
				e.preventDefault();
				first.focus();
			}
		}
		node.addEventListener('keydown', onKey);
		return {
			destroy() {
				node.removeEventListener('keydown', onKey);
				previouslyFocused?.focus?.();
			},
		};
	}
</script>

<main>
	<div class="page-toolbar">
		<div class="search">
			<Icon name="search" class="search__icon" />
			<input type="search" class="search__input"
				bind:value={searchTerm}
				placeholder={t('searchPlaceholder')}
				aria-label={t('searchAriaLabel')} />
		</div>
		<button type="button" class="btn btn--primary" onclick={openAdd}>
			<Icon name="plus" size={16} /> {t('navAddMeal')}
		</button>
	</div>

	{#if loadError}
		<p class="form-error" role="alert">
			<Icon name="circle-alert" size={18} />
			<span>{loadError}</span>
		</p>
	{/if}

	<section class="meal-list-section">
		{#if meals.length === 0}
			{#if searchTerm}
			<div class="no-results">
				<Icon name="search" size={32} class="no-results__icon" />
				<p>{t('noResults', { search: searchTerm })}</p>
			</div>
			{:else}
				<div class="empty-state" data-testid="empty-state">
					<Icon name="empty-meals" size={96} class="empty-state__icon" />
					<h3 class="empty-state__title">{t('emptyStateTitle')}</h3>
					<p>{t('emptyStateDescription')}</p>
				</div>
			{/if}
		{:else}
			<ul class="meal-list">
				{#each meals as meal, i (meal.id)}
					<li
						class:meal-card--editing={editTarget?.id === meal.id}
						in:fly={{ y: -10, duration: tierDuration(250), delay: staggerDuration(i) }}
						out:scale={{ duration: tierDuration(200), start: 0.95 }}
						animate:flip={{ duration: tierDuration(200) }}
					>
						<a
							href="/meals/{meal.id}"
							class="meal-card"
							aria-label={t('mealCardCookAria', { name: meal.name })}
						>
							<div class="meal-card__media">
								{#if meal.has_image}
									<img
										src={mealImageUrl(meal.id)}
										alt={meal.name}
										class="meal-card__hero"
									/>
								{:else}
									<div class="meal-card__placeholder" aria-hidden="true">
										<Icon name="utensils" size={48} />
									</div>
								{/if}
								<div class="meal-card__overlay">
									<button
										type="button"
										class="btn btn--ghost meal-card__action-btn"
										aria-label={t('buttonEdit')}
										onclick={(e) => { e.preventDefault(); e.stopPropagation(); openEdit(meal); }}
									>
										<Icon name="pen-line" size={16} />
									</button>
									<button
										type="button"
										class="btn btn--danger-ghost meal-card__action-btn"
										aria-label={t('buttonDelete')}
										onclick={(e) => { e.preventDefault(); e.stopPropagation(); onDelete(meal); }}
									>
										<Icon name="trash-2" size={16} />
									</button>
								</div>
							</div>
							<div class="meal-card__body">
								<div class="meal-card__header">
									<h3 class="meal-card__name">{meal.name}</h3>
									<span class="meal-card__chip">
										{meal.ingredients.length === 1
											? t('ingredientCountOne')
											: t('ingredientCount', { count: String(meal.ingredients.length) })}
									</span>
								</div>
								<p class="meal-card__ingredients">{ingredientPreview(meal)}</p>
								{#if meal.last_planned_at}
									<p class="meal-card__meta">
										<Icon name="calendar" size={14} />
										{t('lastPlanned', { date: formatDate(meal.last_planned_at, { month: 'short', day: 'numeric' }) })}
									</p>
								{/if}
							</div>
						</a>
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<DeleteConfirmDialog
		open={deleteTarget !== null}
		title={t('buttonDelete')}
		message={t('confirmDelete', { name: deleteTarget?.name ?? '' })}
		confirmLabel={t('buttonDelete')}
		cancelLabel={t('buttonCancel')}
		onconfirm={confirmDeleteDelete}
		oncancel={() => (deleteTarget = null)}
	/>
	{#if editTarget}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="edit-modal-overlay glass--strong" role="dialog" aria-label={t('formEditHeading', { name: editTarget.name || t('formUntitled') })} tabindex="-1" transition:fade={{ duration: tierDuration(200) }} onclick={closeEdit} onkeydown={(e) => { if (e.key === 'Escape') closeEdit(); }} use:focusTrap>
			<div class="edit-modal" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
				<MealForm
					editMode={true}
					editingMeal={editTarget}
					initialName={editTarget.name}
					initialIngredients={editTarget.ingredients.length > 0 ? editTarget.ingredients.map(i => ({ name: i.name, quantity: i.quantity })) : [{ name: '', quantity: null }]}
					initialInstructions={editTarget.instructions}
					submitting={editSubmitting}
					onsubmit={onSubmitEdit}
					oncancel={closeEdit}
				/>
			</div>
		</div>
	{/if}

	{#if addOpen}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="edit-modal-overlay glass--strong" role="dialog" aria-label={t('addMealTitle')} tabindex="-1" transition:fade={{ duration: tierDuration(200) }} onclick={closeAdd} onkeydown={(e) => { if (e.key === 'Escape') closeAdd(); }} use:focusTrap>
			<div class="add-modal" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
				<div class="add-modal__header">
					<h2 class="add-modal__title">{t('addMealTitle')}</h2>
					<button class="add-modal__close" onclick={closeAdd} aria-label={t('lightboxClose')}>
						<Icon name="x" size={20} />
					</button>
				</div>
				<div class="add-modal__body" class:add-modal__body--two-panel={!importCollapsed && importMode !== 'bulk'}>
					<section class="add-modal__panel add-modal__panel--import">
						{#if importCollapsed && importMode !== 'bulk'}
							<div class="import-section--collapsed">
								<Icon name="check" size={18} />
								<span class="import-section__summary">{t('importCollapsedSummary')}</span>
								<button type="button" class="btn btn--ghost" onclick={() => importCollapsed = false}>
									{t('importCollapsedExpand')}
								</button>
							</div>
						{:else}
							<section class="import-card">
								<div class="import-tabs">
									<button type="button" class="import-tab" class:import-tab--active={importMode === 'link'}
										onclick={() => importMode = 'link'}>
										<Icon name="link" size={16} />
										<span>{t('importTabLink')}</span>
									</button>
					<button type="button" class="import-tab" class:import-tab--active={importMode === 'bulk'}
						onclick={() => importMode = 'bulk'}>
						<Icon name="layers" size={16} />
						<span>{t('importTabBulk')}</span>
					</button>
					<button type="button" class="import-tab" class:import-tab--active={importMode === 'llm'}
						onclick={() => importMode = 'llm'}>
						<Icon name="sparkles" size={16} />
						<span>{t('importTabLlm')}</span>
					</button>
								</div>
								{#if importMode === 'link'}
								<label class="import-field">
									<span>{t('importLinkLabel')}</span>
									<textarea bind:value={importInput} placeholder={t('importLinkPlaceholder')} rows="6"></textarea>
								</label>
								<button type="button" class="btn btn--primary" onclick={onImport} disabled={importing || !importInput.trim()}>
									{t('importButtonFetch')}
								</button>
								{:else if importMode === 'bulk'}
								{#if bulkResult}
									<div class="bulk-results">
										<p class="bulk-results__success">
											{t('importBulkResultsSuccess', { count: String(bulkResult.created.length) })}
										</p>
										{#if bulkResult.failed.length > 0}
											<ul class="bulk-results__failures">
												{#each bulkResult.failed as f}
													<li class="form-error">
														<Icon name="circle-alert" size={16} />
														<span class="bulk-results__url">{f.url}</span>
														<span class="bulk-results__reason">{t(f.reason === 'fetch failed' ? 'importBulkReasonFetch' : f.reason === 'no recipe found' ? 'importBulkReasonNoRecipe' : 'importBulkReasonValidation')}</span>
													</li>
												{/each}
											</ul>
										{/if}
										<button type="button" class="btn btn--ghost" onclick={() => { bulkResult = null; bulkUrls = ''; bulkError = null; }}>
											{t('importBulkNewBatch')}
										</button>
									</div>
								{:else}
									<label class="import-field">
										<span>{t('importBulkPlaceholder')}</span>
										<textarea bind:value={bulkUrls} placeholder={t('importBulkPlaceholder')} rows="8" disabled={bulkImporting}></textarea>
									</label>
									<button type="button" class="btn btn--primary" onclick={onBulkImport}
										disabled={bulkImporting || !bulkUrls.trim() || bulkUrls.split('\n').filter(l => l.trim().length > 0).length > 50}>
										{bulkImporting ? t('importButtonBulkLoading') : t('importButtonBulk')}
									</button>
								{/if}
								{:else}
								{#if llmProviders.length === 0 && !llmProvidersLoading}
									<p class="form-error">{t('llmNoProviders')}</p>
								{:else}
									{#if importLlmProvider}
										<div class="llm-settings-toggle">
											{#if llmSettingsCollapsed}
												<span class="llm-settings-summary">
													{t('llmProviderLabel')}: {llmProviders.find(p => p.id === importLlmProvider)?.name ?? importLlmProvider}
													· {t('llmModelLabel')}: {importLlmModel}
												</span>
											{/if}
											<button type="button" class="btn btn--ghost"
												onclick={() => llmSettingsCollapsed = !llmSettingsCollapsed}>
												{llmSettingsCollapsed ? t('llmSettingsChange') : t('llmSettingsHide')}
											</button>
										</div>
									{/if}
									{#if !llmSettingsCollapsed || !importLlmProvider}
										<div class="import-subsection" transition:fly={{ y: -4, duration: 150 }}>
											<div class="llm-provider-row">
												<select bind:value={importLlmProvider} onchange={onProviderChange}
													disabled={llmProvidersLoading || importing}>
													<option value="">{t('llmProviderPlaceholder')}</option>
													{#each llmProviders as p}
														<option value={p.id} disabled={!p.configured && p.id !== 'ollama'}>
															{p.name}{p.configured ? '' : ` (${t('notConfigured')})`}
														</option>
													{/each}
												</select>

												{#if importLlmProvider}
													{#if llmModelsLoading}
														<span class="import-loading">{t('llmModelLoading')}</span>
													{:else if llmModelsError}
														<input type="text" bind:value={importLlmModel} placeholder={t('importLlmModelPlaceholder')} />
													{:else}
														<select bind:value={importLlmModel} disabled={importing}>
															<option value="">{t('llmModelPlaceholder')}</option>
															{#each llmModels as m}
																<option value={m}>{m}</option>
															{/each}
														</select>
													{/if}
												{/if}
											</div>

											{#if importLlmProvider === 'custom'}
												<p class="import-info">{t('llmCustomHint')}</p>
												<label class="import-field">
													<span>{t('llmCustomBaseUrlLabel')}</span>
													<input type="url" bind:value={importLlmCustomBaseUrl} placeholder={t('llmCustomBaseUrlPlaceholder')} />
												</label>
												<label class="import-field">
													<span>{t('llmCustomApiKeyLabel')}</span>
													<input type="password" bind:value={importLlmCustomApiKey} placeholder={t('llmCustomApiKeyPlaceholder')} />
												</label>
											{/if}

											{#if llmModelsError}
												<p class="form-error">{llmModelsError}</p>
											{/if}
											{#if importLlmProvider === 'ollama' && llmModelsError}
												<p class="import-info">{t('llmOllamaHint')}</p>
											{/if}
										</div>
									{/if}

									<textarea
										bind:value={importLlmHint}
										placeholder={t('importLlmHintPlaceholder')}
										rows="6"
										maxlength={20000}
										class="llm-hint-input"
									></textarea>

									<button type="button" class="btn btn--primary" onclick={onImport}
										disabled={importing || !importLlmModel.trim() || !importLlmHint.trim()}>
										{importing ? t('importButtonLlmLoading') : t('importButtonLlm')}
									</button>
								{/if}
								{/if}
								{#if importError || (importMode === 'bulk' && bulkError)}
									<p class="form-error" role="alert">
										<Icon name="circle-alert" size={18} />
										<span>{importMode === 'bulk' && bulkError ? bulkError : importError}</span>
									</p>
								{/if}
							</section>
						{/if}
					</section>
					{#if importMode !== 'bulk'}
						<section class="add-modal__panel add-modal__panel--form">
							{#key importToken}
								<MealForm
									editMode={false}
									initialName={formName}
									initialIngredients={formIngredients}
									initialInstructions={formInstructions}
									initialImage={formImage}
									submitting={submitting}
									onsubmit={onSubmitAdd}
								/>
							{/key}
						</section>
					{/if}
				</div>
			</div>
	</div>
	{/if}
</main>

<style>
	.meal-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.empty-state__title {
		margin: 0;
		font-size: var(--text-xl);
		font-weight: var(--weight-semibold);
		color: var(--color-text);
	}
	.empty-state p {
		margin: 0;
		color: var(--color-text-secondary);
	}

	.no-results__icon {
		display: block;
		margin: 0 auto var(--space-3);
		color: var(--color-text-muted);
	}
	@media (min-width: 768px) {
		.meal-list {
			display: grid;
			grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
			gap: var(--space-5);
		}
	}

	.add-modal {
		max-width: 960px;
		width: 90vw;
		max-height: 88vh;
		overflow: hidden;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		display: flex;
		flex-direction: column;
		box-shadow: var(--shadow-lg);
	}
	.add-modal__header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-4) var(--space-6);
		border-bottom: 1px solid var(--color-border);
		flex-shrink: 0;
	}
	.add-modal__title {
		font-family: var(--font-display);
		font-size: var(--text-xl);
		font-weight: var(--weight-semibold);
		margin: 0;
	}
	.add-modal__close {
		border-radius: var(--radius-full);
		width: 36px;
		height: 36px;
		padding: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		background: transparent;
		border: 0;
		color: var(--color-text-secondary);
		cursor: pointer;
	}
	.add-modal__close:hover {
		background: var(--color-surface-2);
		color: var(--color-text);
	}
	.add-modal__body {
		overflow-y: auto;
		flex: 1;
		padding: var(--space-5) var(--space-6);
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
	}

	.add-modal__body--two-panel {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-6);
		align-items: start;
	}
	.add-modal__panel {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		min-width: 0;
	}

	@media (max-width: 768px) {
		.add-modal {
			max-width: 100vw;
			width: 100vw;
			max-height: 92vh;
		}
		.add-modal__body--two-panel {
			grid-template-columns: 1fr;
			gap: var(--space-5);
		}
	}

	.edit-modal-overlay {
		position: fixed;
		inset: 0;
		z-index: 1000;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: var(--space-4);
		background: var(--glass-scrim-dark);
	}
	.edit-modal {
		max-width: 640px;
		width: 100%;
		max-height: 88vh;
		overflow-y: auto;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		padding: var(--space-6);
	}

	.import-section--collapsed {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		background: var(--color-success-bg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
	}
	.import-section__summary {
		flex: 1;
		font-size: var(--text-sm);
		color: var(--color-text);
	}

	/* Import card — recessive, secondary to authoring form */
	.import-card {
		background: var(--color-surface-2);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		padding: var(--space-5);
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.import-tabs {
		display: flex;
		gap: var(--space-1);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-full);
		padding: var(--space-1);
	}
	.import-tab {
		flex: 1 1 0; min-width: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		border: 0;
		background: transparent;
		color: var(--color-text-secondary);
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		border-radius: var(--radius-full);
		cursor: pointer;
		transition: background var(--motion-morph), color var(--motion-morph);
	}
	.import-tab--active {
		background: var(--color-primary);
		color: var(--color-on-primary);
	}
	.import-tab:hover:not(.import-tab--active) {
		background: var(--color-surface-2);
		color: var(--color-text);
	}

	.bulk-results {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.bulk-results__success {
		margin: 0;
		font-weight: var(--weight-semibold);
		color: var(--color-success);
	}
	.bulk-results__failures {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.bulk-results__url {
		font-size: var(--text-xs);
		color: var(--color-text-muted);
		word-break: break-all;
	}
	.bulk-results__reason {
		font-size: var(--text-xs);
		color: var(--color-error);
	}

	@media (max-width: 768px) {
		.import-tabs {
			flex-wrap: wrap;
			overflow-x: visible;
			border-radius: var(--radius-lg);
		}
		.import-tab {
			flex: 1 1 calc(50% - var(--space-1));
			white-space: normal;
		}
		.import-tab span {
			font-size: var(--text-xs);
		}
	}


	.import-subsection {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
	}

    .llm-settings-toggle {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--space-3);
        flex-wrap: wrap;
    }
    .llm-settings-summary {
        color: var(--color-text-secondary);
        font-size: var(--text-sm);
        text-wrap: pretty;
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
    .llm-hint-input {
        width: 100%;
        min-height: 140px;
        resize: vertical;
    }
</style>
