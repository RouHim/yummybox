<script lang="ts">
	import { getMeal, updateMeal, deleteMeal, mealImageUrl, polishInstructions, ApiError, listMeals } from '$lib/api';
	import Icon from '$lib/Icon.svelte';
	import { t } from '$lib/i18n';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import type { Meal, NewIngredientLine } from '$lib/types';
	import { fade } from 'svelte/transition';
	import { tierDuration } from '$lib/motion';
	import DeleteConfirmDialog from '$lib/DeleteConfirmDialog.svelte';
import { focusTrap } from '$lib/focusTrap';
	import CookingView from '$lib/components/CookingView.svelte';
	import { readStoredLlmConfig } from '$lib/llm-config.svelte';
	import MealForm from '$lib/MealForm.svelte';

	let meal = $state<Meal | null>(null);
	let loading = $state(true);
	let notFound = $state(false);
	let loadError = $state<string | null>(null);
	const mealId = $derived(Number(page.params.id));
	let allMeals = $state<Meal[]>([]);
	let existingMealNames = $derived(
		new Set(allMeals.map(m => m.name.trim().toLowerCase().split(/\s+/).join(' ')))
	);

	let deleteOpen = $state(false);
	let deleting = $state(false);
	let deleteError = $state<string | null>(null);

	let editOpen = $state(false);
	let editSubmitting = $state(false);
	let polishing = $state(false);
	let polishError = $state<string | null>(null);

	let hasLlmConfig = $derived.by(() => {
		const config = readStoredLlmConfig();
		return !!config && !!config.model;
	});

	async function loadMeal() {
		loading = true;
		notFound = false;
		loadError = null;
		try {
			meal = await getMeal(mealId);
			try { allMeals = await listMeals(); } catch { /* best-effort */ }
		} catch (err) {
			meal = null;
			notFound = true;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (!Number.isNaN(mealId)) loadMeal();
	});

	function openDelete() { deleteOpen = true; deleteError = null; }
	function closeDelete() { deleteOpen = false; deleteError = null; }

	async function confirmDelete() {
		if (!meal) return;
		deleting = true;
		deleteError = null;
		try {
			await deleteMeal(meal.id);
			deleteOpen = false;
			await goto('/meals');
		} catch (err) {
			deleteError = err instanceof ApiError && err.code === 'REQUEST_FAILED'
				? t('errorDeleteFailed')
				: err instanceof Error ? err.message : String(err);
		} finally {
			deleting = false;
		}
	}

	function editMeal() {
		if (!meal) return;
		editOpen = true;
	}

	function closeEdit() {
		editOpen = false;
	}

	async function onSubmitEdit(payload: {
		name: string; ingredients: NewIngredientLine[]; instructions: string;
		portions: number | null; source_url: string | null; image: File | null; removeImage: boolean;
	}) {
		if (!meal) return;
		editSubmitting = true;
		try {
			await updateMeal(meal.id, { name: payload.name, ingredients: payload.ingredients, instructions: payload.instructions, portions: payload.portions, source_url: payload.source_url }, {
				image: payload.image,
				removeImage: payload.removeImage,
			});
			await loadMeal();
			closeEdit();
		} finally {
			editSubmitting = false;
		}
	}


	async function doPolish() {
		if (!meal || polishing) return;
		const config = readStoredLlmConfig();
		if (!config || !config.model) return;
		polishing = true;
		polishError = null;
		try {
			const polished = await polishInstructions(
				config.model,
				meal.name,
				meal.ingredients,
				meal.instructions,
				config.provider === 'custom' ? config.customBaseUrl : undefined,
				config.provider === 'custom' ? config.customApiKey : undefined,
			);
			await updateMeal(meal.id, {
				name: meal.name,
				ingredients: meal.ingredients,
				instructions: polished,
				portions: meal.portions ?? null,
				source_url: meal.source_url ?? null,
			});
			await loadMeal();
		} catch (err) {
			if (err instanceof ApiError) {
				if (err.code === 'llm_timeout') polishError = t('llmErrorTimeout');
				else if (err.code === 'llm_parse_failed') polishError = t('llmErrorParseFailed');
				else if (err.code === 'llm_api_key_missing') polishError = t('llmErrorApiKey', { envVar: '' });
				else polishError = t('polishErrorFailed');
			} else {
				polishError = t('polishErrorFailed');
			}
		} finally {
			polishing = false;
		}
	}
</script>

<main>
	{#if loading}
		<p class="cooking-view__loading">Loading...</p>
	{:else if notFound}
		<p class="cooking-view__not-found">{t('cookingViewNotFound')}</p>
	{:else if meal}
		{#key meal.id}
			<CookingView
				{meal}
				imageUrl={meal.has_image ? mealImageUrl(meal.id) : null}
				plannedAt={meal.last_planned_at}
				{polishError}
			>
				{#snippet heroActions()}
					<button
						type="button"
						class="btn btn--ghost cooking-view__action-btn"
						aria-label={t('buttonEdit')}
						title={t('buttonEdit')}
						onclick={editMeal}
						disabled={deleting}
					>
						<Icon name="pen-line" size={16} />
					</button>
					{#if hasLlmConfig}
						<button
							type="button"
							class="btn btn--ghost cooking-view__action-btn"
							aria-label={t('buttonPolish')}
							title={t('buttonPolish')}
							onclick={doPolish}
							disabled={polishing || deleting}
						>
							{#if polishing}
								<Icon name="loader-circle" size={16} spin={true} />
							{:else}
								<Icon name="sparkles" size={16} />
							{/if}
						</button>
					{/if}
					<button
						type="button"
						class="btn btn--danger-ghost cooking-view__action-btn"
						aria-label={t('buttonDelete')}
						title={t('buttonDelete')}
						onclick={openDelete}
						disabled={deleting}
					>
						<Icon name="trash-2" size={16} />
					</button>
				{/snippet}
			</CookingView>
		{/key}
	{/if}


	{#if editOpen && meal}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="edit-modal-overlay glass--strong" role="dialog" aria-label={t('formEditHeading', { name: meal.name || t('formUntitled') })} tabindex="-1" transition:fade={{ duration: tierDuration(200) }} onclick={closeEdit} onkeydown={(e) => { if (e.key === 'Escape') closeEdit(); }} ondragover={(e) => e.preventDefault()} ondrop={(e) => e.preventDefault()} use:focusTrap>
		<div class="edit-modal" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
				<MealForm
					editMode={true}
					editingMeal={meal}
					initialName={meal.name}
					initialIngredients={meal.ingredients.length > 0 ? meal.ingredients.map(i => ({ name: i.name, quantity: i.quantity })) : [{ name: '', quantity: null }]}
					initialInstructions={meal.instructions}
					initialPortions={meal.portions ?? null}
					initialSourceUrl={meal.source_url ?? null}
					submitting={editSubmitting}
					existingNames={existingMealNames}
					onsubmit={onSubmitEdit}
					oncancel={closeEdit}
				/>
			</div>
		</div>
	{/if}
	<DeleteConfirmDialog
		open={deleteOpen}
		title={t('buttonDelete')}
		message={t('confirmDelete', { name: meal?.name ?? '' })}
		confirmLabel={t('buttonDelete')}
		cancelLabel={t('buttonCancel')}
		onconfirm={confirmDelete}
		oncancel={closeDelete}
	/>
	{#if deleteError}
		<p class="form-error delete-error" role="alert">
			<Icon name="circle-alert" size={18} />
			<span>{deleteError}</span>
		</p>
	{/if}
</main>

<style>
	.delete-error {
		position: fixed;
		top: var(--space-4);
		left: 50%;
		transform: translateX(-50%);
		z-index: 1002;
		max-width: min(28rem, calc(100vw - 2 * var(--space-4)));
		width: max-content;
	}
</style>
