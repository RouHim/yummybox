<script lang="ts">
	import Icon from '$lib/Icon.svelte';
	import { t, formatDate } from '$lib/i18n';
	import type { Snippet } from 'svelte';
	import type { Meal } from '$lib/types';
	import { fly } from 'svelte/transition';
	import { tierDuration } from '$lib/motion';

	let {
		meal,
		imageUrl = null,
		plannedAt = undefined,
		polishError = null,
		heroActions,
	}: {
		meal: Meal;
		imageUrl?: string | null;
		plannedAt?: string | null;
		polishError?: string | null;
		heroActions?: Snippet;
	} = $props();

	let desiredPortions = $state<number | null>(null);
	$effect(() => {
		desiredPortions = meal.portions ?? null;
	});

	function scaleQuantity(quantity: string | null, base: number, desired: number): string | null {
		if (!quantity || desired <= 0 || desired === base) return null;
		const match = quantity.match(/^(\d+\.?\d*)/);
		if (!match) return null;
		const num = parseFloat(match[1]);
		const scaled = num * (desired / base);
		const formatted = scaled % 1 === 0 ? scaled.toFixed(0) : scaled.toFixed(1);
		return quantity.replace(/^\d+\.?\d*/, formatted);
	}
</script>

<article class="cooking-view" in:fly={{ y: 8, duration: tierDuration(250) }}>

	<figure class="cooking-view__hero">
		{#if imageUrl}
			<img
				src={imageUrl}
				alt={meal.name}
				class="cooking-view__hero-img"
			/>
		{:else}
			<div class="cooking-view__hero-placeholder" aria-hidden="true">
				<Icon name="utensils" size={48} />
			</div>
		{/if}
		{#if heroActions}
			<div class="cooking-view__hero-overlay">
				{@render heroActions()}
			</div>
		{/if}
	</figure>


	{#if polishError}
		<p class="cooking-view__polish-error" role="alert">{polishError}</p>
	{/if}
	<header class="cooking-view__header">
		<h1 class="cooking-view__name">{meal.name}</h1>

		<p class="cooking-view__meta">
			<span>{meal.ingredients.length === 1 ? t('ingredientCountOne') : t('ingredientCount', { count: String(meal.ingredients.length) })}</span>
			{#if plannedAt !== undefined}
				<span class="cooking-view__meta-sep" aria-hidden="true">·</span>
				<span>{plannedAt ? t('lastPlanned', { date: formatDate(plannedAt, { month: 'short', day: 'numeric', year: 'numeric' }) }) : t('lastPlannedNever')}</span>
			{/if}
		</p>

	{#if meal.portions != null}
		{@const p = meal.portions}
		<div class="cooking-view__servings">
			<span class="cooking-view__servings-label">{t('cookingViewServes', { count: String(p) })}</span>
			<span class="cooking-view__stepper">
				<button
					type="button" class="cooking-view__stepper-btn"
					aria-label={t('cookingViewDecrement')}
					onclick={() => desiredPortions = Math.max(1, (desiredPortions ?? p) - 1)}
					disabled={(desiredPortions ?? p) <= 1}
				>&minus;</button>
				<span class="cooking-view__stepper-value">{desiredPortions ?? p}</span>
				<button
					type="button" class="cooking-view__stepper-btn"
					aria-label={t('cookingViewIncrement')}
					onclick={() => desiredPortions = Math.min(10000, (desiredPortions ?? p) + 1)}
					disabled={(desiredPortions ?? p) >= 10000}
				>+</button>
			</span>
		</div>
	{/if}
	</header>

	<div class="cooking-view__body">
		<section class="cooking-view__ingredients">
			<h2 class="cooking-view__section-title">{t('cookingViewIngredients')}</h2>
			<ul class="cooking-view__ingredient-list">
				{#each meal.ingredients as ingredient (ingredient.name)}
					{@const scaling = meal.portions != null && desiredPortions != null && desiredPortions > 0 && desiredPortions !== meal.portions}
					{@const scaled = scaling ? scaleQuantity(ingredient.quantity, meal.portions!, desiredPortions!) : null}
					<li>
						<span>{ingredient.name}</span>
						{#if ingredient.quantity}
							<span class="cooking-view__qty" class:cooking-view__qty--muted={scaling}>{ingredient.quantity}</span>
						{/if}
						{#if scaled}
							<span class="cooking-view__qty cooking-view__qty--scaled">{scaled}</span>
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

	.cooking-view__servings {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		margin-top: var(--space-3);
	}

	.cooking-view__servings-label {
		font-size: var(--text-sm);
		color: var(--color-text-muted);
		font-family: var(--font-sans);
	}

	.cooking-view__stepper {
		display: inline-flex;
		align-items: center;
		gap: 0;
		border-radius: var(--radius-full);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		overflow: hidden;
	}

	.cooking-view__stepper-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 36px;
		height: 36px;
		padding: 0;
		border: none;
		background: transparent;
		color: var(--color-text-secondary);
		font-size: var(--text-lg);
		font-family: var(--font-sans);
		line-height: 1;
		cursor: pointer;
		transition: background var(--transition-fast), color var(--transition-fast);
	}

	.cooking-view__stepper-btn:hover {
		background: var(--color-surface-2);
		color: var(--color-text);
	}

	.cooking-view__stepper-btn:active {
		transform: scale(0.92);
	}

	.cooking-view__stepper-btn:disabled {
		color: var(--color-text-muted);
		cursor: default;
		background: transparent;
	}

	.cooking-view__stepper-value {
		display: flex;
		align-items: center;
		justify-content: center;
		min-width: 40px;
		height: 36px;
		padding: 0 var(--space-2);
		font-size: var(--text-base);
		font-weight: var(--weight-semibold);
		font-family: var(--font-sans);
		color: var(--color-primary);
		border-left: 1px solid var(--color-border-light);
		border-right: 1px solid var(--color-border-light);
	}

	.cooking-view__qty--muted {
		color: var(--color-text-muted);
		text-decoration: line-through;
	}

	.cooking-view__qty--scaled {
		color: var(--color-primary);
		font-weight: var(--weight-medium);
	}
</style>