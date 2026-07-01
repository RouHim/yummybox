<script lang="ts">
	import { getMeal, deleteMeal, mealImageUrl } from '$lib/api';
	import Icon from '$lib/Icon.svelte';
	import { t, formatDate } from '$lib/i18n';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import type { Meal } from '$lib/types';
	import { fly } from 'svelte/transition';
	import { tierDuration } from '$lib/motion';
	import DeleteConfirmDialog from '$lib/DeleteConfirmDialog.svelte';

	let meal = $state<Meal | null>(null);
	let loading = $state(true);
	let notFound = $state(false);
	let loadError = $state<string | null>(null);
	const mealId = $derived(Number(page.params.id));

	let deleteOpen = $state(false);
	let deleting = $state(false);

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
</script>

<main>
	{#if loading}
		<p class="cooking-view__loading">Loading...</p>
	{:else if notFound}
		<p class="cooking-view__not-found">{t('cookingViewNotFound')}</p>
		<a href="/meals" class="nav-link"><Icon name="utensils" size={16} /> {t('cookingViewBack')}</a>
	{:else if meal}
		<article class="cooking-view" in:fly={{ y: 8, duration: tierDuration(250) }}>
			<a href="/meals" class="cooking-view__back nav-link">
				<Icon name="utensils" size={16} /> {t('cookingViewBack')}
			</a>

			{#if meal.has_image}
				<figure class="cooking-view__hero">
					<img
						src={mealImageUrl(meal.id)}
						alt={meal.name}
						class="cooking-view__hero-img"
					/>
				</figure>
			{/if}

			<header class="cooking-view__header">
				<h1 class="cooking-view__name">{meal.name}</h1>

				<p class="cooking-view__meta">
					<span>{meal.ingredients.length === 1 ? t('ingredientCountOne') : t('ingredientCount', { count: String(meal.ingredients.length) })}</span>
					<span class="cooking-view__meta-sep" aria-hidden="true">·</span>
					<span>{meal.last_planned_at ? t('lastPlanned', { date: formatDate(meal.last_planned_at, { month: 'short', day: 'numeric', year: 'numeric' }) }) : t('lastPlannedNever')}</span>
				</p>

				<div class="cooking-view__actions">
					<button type="button" class="btn btn--ghost" onclick={editMeal} disabled={deleting}>
						<Icon name="pen-line" size={16} /> {t('cookingViewEditMeal')}
					</button>
					<button type="button" class="btn btn--danger-ghost" onclick={openDelete} disabled={deleting}>
						<Icon name="trash-2" size={16} /> {t('cookingViewDeleteMeal')}
					</button>
				</div>
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
</style>
