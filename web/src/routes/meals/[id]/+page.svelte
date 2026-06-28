<script lang="ts">
	import { getMeal, mealImageUrl } from '$lib/api';
	import Icon from '$lib/Icon.svelte';
	import { t } from '$lib/i18n';
	import { page } from '$app/state';
	import type { Meal } from '$lib/types';

	let meal = $state<Meal | null>(null);
	let loading = $state(true);
	let notFound = $state(false);
	let loadError = $state<string | null>(null);
	const mealId = $derived(Number(page.params.id));

	async function loadMeal() {
		loading = true;
		notFound = false;
		loadError = null;
		try {
			meal = await getMeal(mealId);
		} catch (err) {
			// getMeal throws on any non-ok response; the only non-ok for GET /api/meals/:id is 404
			meal = null;
			notFound = true;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (!Number.isNaN(mealId)) loadMeal();
	});
</script>

<main>
	{#if loading}
		<p class="cooking-view__loading">Loading...</p>
	{:else if notFound}
		<p class="cooking-view__not-found">{t('cookingViewNotFound')}</p>
		<a href="/meals" class="nav-link"><Icon name="utensils" size={16} /> {t('cookingViewBack')}</a>
	{:else if meal}
		<div class="detail-wrapper glass">
			<header class="page-header">
				<h1>{t('cookingViewTitle')}</h1>
				<div class="page-header__right">
				<a href="/meals" class="nav-link"><Icon name="utensils" size={16} /> {t('navMeals')}</a>
				<a href="/planner" class="nav-link"><Icon name="calendar" size={16} /> {t('navPlanner')}</a>
			</div>
		</header>

		<h2 class="cooking-view__name">{meal.name}</h2>

		{#if meal.has_image}
			<img
				src={mealImageUrl(meal.id)}
				alt={meal.name}
				class="cooking-view__image"
			/>
		{/if}

		<section class="cooking-view__ingredients">
			<h3>{t('cookingViewIngredients')}</h3>
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
				<h3>{t('fieldInstructionsLabel')}</h3>
				<p class="cooking-view__instructions-text">{meal.instructions}</p>
			</section>
		{/if}

		<a href="/meals" class="nav-link"><Icon name="utensils" size={16} /> {t('cookingViewBack')}</a>
		</div>
	{/if}
</main>

<style>
	.page-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--space-3);
		margin-bottom: var(--space-4);
	}
	.detail-wrapper {
		padding: var(--space-6);
		border-radius: var(--radius-lg);
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.page-header__right {
		display: flex;
		align-items: center;
		gap: var(--space-3);
	}

	.nav-link {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		color: var(--color-primary);
		text-decoration: none;
		font-weight: var(--weight-medium);
		font-size: var(--text-base);
	}
	.nav-link:hover { text-decoration: underline; }

	.cooking-view__loading {
		color: var(--color-text-secondary);
		font-style: italic;
	}

	.cooking-view__not-found {
		color: var(--color-text-secondary);
		font-size: var(--text-lg);
		margin-bottom: var(--space-4);
	}

	.cooking-view__name {
		font-family: var(--font-display);
		margin-top: var(--space-4);
		margin-bottom: var(--space-4);
		font-size: var(--text-2xl);
		font-weight: var(--weight-semibold);
	}

	.cooking-view__image {
		max-width: 100%;
		border-radius: var(--radius-lg);
		margin-bottom: var(--space-6);
		display: block;
	}

	.cooking-view__ingredients {
		margin-bottom: var(--space-6);
	}
	.cooking-view__ingredients h3 {
		font-size: var(--text-sm);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--color-text-secondary);
		margin-bottom: var(--space-3);
	}

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
	.cooking-view__instructions {
		margin-bottom: var(--space-6);
	}
	.cooking-view__instructions h3 {
		font-size: var(--text-sm);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--color-text-secondary);
		margin-bottom: var(--space-3);
	}
	.cooking-view__instructions-text {
		white-space: pre-wrap;
		line-height: 1.6;
	}
</style>
