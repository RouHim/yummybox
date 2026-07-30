<script lang="ts">
    import { getPlan, mealImageUrl, ApiError } from '$lib/api';
	import { weekOfDate, mondaySundayOf } from '$lib/week';
	import { t } from '$lib/i18n';
	import { fly, scale } from 'svelte/transition';
	import { tierDuration, staggerDuration } from '$lib/motion';
	import { goto } from '$app/navigation';
	import type { Plan } from '$lib/types';
	import Icon from '$lib/Icon.svelte';

	let plan = $state<Plan | null>(null);
	let loading = $state(true);
	let loadError = $state<string | null>(null);
	const currentWeekInfo = weekOfDate(new Date());

	function formatDateRange(): string {
		const { monday, sunday } = mondaySundayOf(currentWeekInfo.year, currentWeekInfo.week);
		const opts: Intl.DateTimeFormatOptions = { month: 'short', day: 'numeric' };
		return `${monday.toLocaleDateString(undefined, opts)} to ${sunday.toLocaleDateString(undefined, opts)}`;
	}

	async function loadCurrentWeek() {
		loading = true;
		loadError = null;
		try {
			plan = await getPlan(currentWeekInfo.year, currentWeekInfo.week);
			if (plan === null) {
				await goto('/planner?focus=current');
				return;
			}
		} catch (err) {
			loadError = err instanceof ApiError && err.code === 'REQUEST_FAILED'
				? t('errorLoadPlan')
				: err instanceof Error ? err.message : '';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		loadCurrentWeek();
	});
</script>

<main>
	{#if loading}
		<p class="week-meals__loading">Loading...</p>
	{:else if loadError}
		<p class="form-error" role="alert">
			<Icon name="circle-alert" size={18} />
			<span>{loadError}</span>
		</p>
		<button class="btn btn--ghost" onclick={loadCurrentWeek}>{t('buttonRetry')}</button>
	{:else if plan}
		<header class="week-header">
			<h1 class="week-header__title">{t('currentWeekMeals')}</h1>
			<p class="week-header__range">{formatDateRange()}</p>
		</header>

		<section class="week-meals">
			{#if plan.meals.length === 0}
				<div class="week-empty">
					<Icon name="empty-meals" size={48} />
					<p class="week-empty__msg">{t('currentWeekNoMeals')}</p>
					<a href="/planner?focus=current" class="btn btn--ghost">
						<Icon name="calendar" size={16} /> {t('currentWeekPlannerLink')}
					</a>
				</div>
			{:else}
				<ul class="week-meal-list">
					{#each plan.meals as meal, i (meal.id)}
						<li
							in:fly={{ y: 12, duration: tierDuration(250), delay: staggerDuration(i) }}
							out:scale={{ duration: tierDuration(200), start: 0.95 }}
						>
							<a href="/meals/{meal.id}" class="week-meal-card" aria-label={t('mealCardCookAria', { name: meal.name })}>
								<div class="week-meal-card__media">
									{#if meal.has_image}
										<img src={mealImageUrl(meal.id)} alt={meal.name} class="week-meal-card__img" />
									{:else}
										<div class="week-meal-card__placeholder" aria-hidden="true">
											<Icon name="utensils" size={48} />
										</div>
									{/if}
								</div>
								<div class="week-meal-card__body">
									<h2 class="week-meal-card__name">{meal.name}</h2>
								</div>
							</a>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

	{/if}
</main>


<style>

	.week-header {
		margin-bottom: var(--space-6);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		padding: var(--space-4) var(--space-5);
	}
	.week-header__title {
		font-family: var(--font-display);
		font-size: var(--text-2xl);
		font-weight: var(--weight-semibold);
		margin: 0 0 var(--space-1) 0;
		line-height: 1.15;
	}
	.week-header__range {
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
		margin: 0;
	}

	.week-meals__loading {
		color: var(--color-text-secondary);
		font-style: italic;
	}

	.week-meal-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: grid;
		grid-template-columns: 1fr;
		gap: var(--space-4);
	}

	@media (min-width: 768px) {
		.week-meal-list {
			grid-template-columns: repeat(auto-fill, 300px);
			justify-content: center;
		}
	}
	.week-meal-card {
		display: block;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		overflow: hidden;
		text-decoration: none;
		color: inherit;
		transition: box-shadow var(--transition-base), border-color var(--transition-base);
	}
	.week-meal-card:hover {
		box-shadow: var(--shadow-md);
		border-color: var(--color-border-strong);
	}
	.week-meal-card__media {
		aspect-ratio: 16 / 9;
		background: var(--color-surface-2);
	}
	.week-meal-card__img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.week-meal-card__placeholder {
		width: 100%;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-text-muted);
	}
	.week-meal-card__body {
		padding: var(--space-3);
	}
	.week-meal-card__name {
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--weight-semibold);
		margin: 0 0 var(--space-1) 0;
		line-height: 1.2;
	}

	.week-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-8) var(--space-4);
		text-align: center;
		color: var(--color-text-muted);
	}
	.week-empty__msg {
		margin: 0;
		color: var(--color-text-secondary);
	}


</style>
