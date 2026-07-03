<script lang="ts">
	import { getMeal, updateMeal, deleteMeal, mealImageUrl, polishInstructions, ApiError, listMeals } from '$lib/api';
	import Icon from '$lib/Icon.svelte';
	import { t, formatDate } from '$lib/i18n';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import type { Meal, NewIngredientLine } from '$lib/types';
	import { fly, fade } from 'svelte/transition';
	import { tierDuration } from '$lib/motion';
	import DeleteConfirmDialog from '$lib/DeleteConfirmDialog.svelte';
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

	function openDelete() { deleteOpen = true; }
	function closeDelete() { deleteOpen = false; }

	async function confirmDelete() {
		if (!meal) return;
		deleting = true;
		try {
			await deleteMeal(meal.id);
			deleteOpen = false;
			await goto('/meals');
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
		image: File | null; removeImage: boolean;
	}) {
		if (!meal) return;
		editSubmitting = true;
		try {
			await updateMeal(meal.id, { name: payload.name, ingredients: payload.ingredients, instructions: payload.instructions }, {
				image: payload.image,
				removeImage: payload.removeImage,
			});
			await loadMeal();
			closeEdit();
		} finally {
			editSubmitting = false;
		}
	}

	// matches DeleteConfirmDialog.svelte and meals/+page.svelte focusTrap
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
		<article class="cooking-view" in:fly={{ y: 8, duration: tierDuration(250) }}>

			<figure class="cooking-view__hero">
				{#if meal.has_image}
					<img
						src={mealImageUrl(meal.id)}
						alt={meal.name}
						class="cooking-view__hero-img"
					/>
				{:else}
					<div class="cooking-view__hero-placeholder" aria-hidden="true">
						<Icon name="utensils" size={48} />
					</div>
				{/if}
				<div class="cooking-view__hero-overlay">
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
				</div>
			</figure>


			{#if polishError}
				<p class="cooking-view__polish-error" role="alert">{polishError}</p>
			{/if}
			<header class="cooking-view__header">
				<h1 class="cooking-view__name">{meal.name}</h1>

				<p class="cooking-view__meta">
					<span>{meal.ingredients.length === 1 ? t('ingredientCountOne') : t('ingredientCount', { count: String(meal.ingredients.length) })}</span>
					<span class="cooking-view__meta-sep" aria-hidden="true">·</span>
					<span>{meal.last_planned_at ? t('lastPlanned', { date: formatDate(meal.last_planned_at, { month: 'short', day: 'numeric', year: 'numeric' }) }) : t('lastPlannedNever')}</span>
				</p>
			</header>

			<div class="cooking-view__body">
				<section class="cooking-view__ingredients">
					<h2 class="cooking-view__section-title">{t('cookingViewIngredients')}</h2>
					<ul class="cooking-view__ingredient-list">
						{#each meal.ingredients as ingredient (ingredient.name)}
							<li>
								<span>{ingredient.name}</span>
								{#if ingredient.quantity}
									<span class="cooking-view__qty">{ingredient.quantity}</span>
								{/if}
							</li>
						{/each}
					</ul>
				</section>

				{#if meal.instructions}
					<section class="cooking-view__instructions">
						<h2 class="cooking-view__section-title">{t('fieldInstructionsLabel')}</h2>
						<div class="cooking-view__instructions-text">{@html meal.instructions}</div>
					</section>
				{/if}
			</div>
		</article>
	{/if}


	{#if editOpen && meal}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="edit-modal-overlay glass--strong" role="dialog" aria-label={t('formEditHeading', { name: meal.name || t('formUntitled') })} tabindex="-1" transition:fade={{ duration: tierDuration(200) }} onclick={closeEdit} onkeydown={(e) => { if (e.key === 'Escape') closeEdit(); }} use:focusTrap>
			<div class="edit-modal" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
				<MealForm
					editMode={true}
					editingMeal={meal}
					initialName={meal.name}
					initialIngredients={meal.ingredients.length > 0 ? meal.ingredients.map(i => ({ name: i.name, quantity: i.quantity })) : [{ name: '', quantity: null }]}
					initialInstructions={meal.instructions}
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
</main>

<style>
	.cooking-view__ingredient-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.cooking-view__ingredient-list li {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		padding: var(--space-2) 0;
		border-bottom: 1px solid var(--color-border-light, var(--color-border));
	}
	.cooking-view__ingredient-list li:last-child {
		border-bottom: none;
	}

	.cooking-view__qty {
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
		font-style: italic;
	}

	.cooking-view__instructions-text {
		white-space: pre-wrap;
		line-height: 1.6;
	}

	.cooking-view__polish-error {
		color: var(--color-error);
		font-size: var(--text-sm);
		padding: var(--space-2) var(--space-3);
		margin: 0 var(--space-3);
	}
</style>
