<script lang="ts">
    import { getPlan, mealImageUrl, ApiError } from '$lib/api';
	import { weekOfDate, formatWeekRange } from '$lib/week';
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
		<p class="view-loading">{t('loadingLabel')}</p>
	{:else if loadError}
		<p class="form-error" role="alert">
			<Icon name="circle-alert" size={18} />
			<span>{loadError}</span>
		</p>
		<button class="btn btn--ghost" onclick={loadCurrentWeek}>{t('buttonRetry')}</button>
	{:else if plan}
		<header class="page-hero glass">
			<h1 class="page-hero__title">{t('currentWeekMeals')}</h1>
			<p class="page-hero__sub">{formatWeekRange(currentWeekInfo.year, currentWeekInfo.week)}</p>
		</header>

		<section class="week-meals">
			{#if plan.meals.length === 0}
				<div class="empty-state">
					<Icon name="empty-meals" size={96} class="empty-state__icon" />
					<p>{t('currentWeekNoMeals')}</p>
					<a href="/planner?focus=current" class="btn btn--ghost">
						<Icon name="calendar" size={16} /> {t('currentWeekPlannerLink')}
					</a>
				</div>
			{:else}
				<ul class="meal-grid">
					{#each plan.meals as meal, i (meal.id)}
						<li
							class="meal-card"
							in:fly={{ y: 12, duration: tierDuration(250), delay: staggerDuration(i) }}
							out:scale={{ duration: tierDuration(200), start: 0.95 }}
						>
							<div class="meal-card__media">
								{#if meal.has_image}
									<img src={mealImageUrl(meal.id)} alt={meal.name} class="meal-card__hero" />
								{:else}
									<div class="meal-card__placeholder" aria-hidden="true">
										<Icon name="utensils" size={48} />
									</div>
								{/if}
							</div>
							<div class="meal-card__body">
								<h2 class="meal-card__name">{meal.name}</h2>
							</div>
							<a
								class="meal-card__cover"
								href="/meals/{meal.id}"
								aria-label={t('mealCardCookAria', { name: meal.name })}
							></a>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

	{/if}
</main>
