<script lang="ts">
	import { getMeal, updateMeal, deleteMeal, mealImageUrl, polishInstructions, ApiError } from '$lib/api';
	import Icon from '$lib/Icon.svelte';
	import { t, formatDate } from '$lib/i18n';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import type { Meal } from '$lib/types';
	import { fly } from 'svelte/transition';
	import { tierDuration } from '$lib/motion';
	import DeleteConfirmDialog from '$lib/DeleteConfirmDialog.svelte';
	import { readStoredLlmConfig } from '$lib/llm-config.svelte';

	let meal = $state<Meal | null>(null);
	let loading = $state(true);
	let notFound = $state(false);
	let loadError = $state<string | null>(null);
	const mealId = $derived(Number(page.params.id));

	let deleteOpen = $state(false);
	let deleting = $state(false);
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
		goto('/meals?edit=' + meal.id);
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
