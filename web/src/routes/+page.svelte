<script lang="ts">
	import { getPlan, mealImageUrl } from '$lib/api';
	import { weekOfDate, mondaySundayOf } from '$lib/week';
	import { t } from '$lib/i18n';
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
		return `${monday.toLocaleDateString(undefined, opts)} – ${sunday.toLocaleDateString(undefined, opts)}`;
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
			const raw = err instanceof Error ? err.message : '';
			loadError = raw === '__REQUEST_FAILED__' ? t('errorLoadPlan') : raw;
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
		<p class="current-week__loading">Loading...</p>
	{:else if loadError}
		<p class="form-error" role="alert">
			<Icon name="circle-alert" size={18} />
			<span>{loadError}</span>
		</p>
		<button class="btn btn--ghost" onclick={loadCurrentWeek}>{t('buttonRetry')}</button>
	{:else if plan}
		<header class="page-header glass">
			<div>
				<h1>{t('currentWeekTitle')}</h1>
				<p class="page-header__subtitle">{formatDateRange()}</p>
			</div>
			<div class="page-header__right">
				<a href="/meals" class="nav-link"><Icon name="utensils" size={16} /> {t('navMeals')}</a>
				<a href="/planner" class="nav-link">
					<Icon name="calendar" size={16} />
					{t('navPlanner')}
				</a>
			</div>
		</header>

		<section class="current-week-meals">
			<h2>{t('currentWeekMeals')}</h2>
			{#if plan.meals.length === 0}
				<p class="current-week__empty-msg">{t('currentWeekNoMeals')}</p>
			{:else}
				<ul class="current-week-meal-list">
					{#each plan.meals as meal (meal.id)}
						<li class="current-week-meal">
							<a href="/meals/{meal.id}" class="current-week-meal__link">
								{#if meal.has_image}
									<img
										src={mealImageUrl(meal.id)}
										alt={meal.name}
										class="current-week-meal__img"
									/>
								{/if}
								<div class="current-week-meal__info">
									<strong class="current-week-meal__name">{meal.name}</strong>
									<span class="current-week-meal__ings">
										{meal.ingredients.map(i => i.quantity ? `${i.name} (${i.quantity})` : i.name).join(', ')}
									</span>
								</div>
							</a>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		{#if plan.ingredient_summary.length > 0}
			<section class="current-week-summary">
				<h2>{t('currentWeekIngredientSummary')}</h2>
				<ul class="summary-list">
					{#each plan.ingredient_summary as entry (entry.name)}
						<li class="summary-item">
							<span class="summary-item__name">{entry.name}</span>
							{#if entry.numeric_total}
								<span class="summary-item__num">
									{entry.numeric_total.value}
									{#if entry.numeric_total.unit} {entry.numeric_total.unit}{/if}
								</span>
							{/if}
							{#each entry.non_numeric as qty}
								<span class="summary-item__text">{qty}</span>
							{/each}
						</li>
					{/each}
				</ul>
			</section>
		{/if}
	{/if}
</main>


<style>
	.page-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--space-3);
		position: sticky;
		top: 0;
		z-index: 20;
		padding: var(--space-3) var(--space-4);
		border-radius: 0;
		border-left: none;
		border-right: none;
		border-top: none;
	}
	.page-header.glass .nav-link {
		background: var(--glass-scrim);
		border-radius: var(--radius-full);
		padding: var(--space-1) var(--space-2);
	}
	.page-header__subtitle {
		margin: 0;
		color: var(--color-text-secondary);
		font-size: var(--text-base);
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

	.current-week__loading {
		color: var(--color-text-secondary);
		font-style: italic;
	}

	.current-week__empty-msg {
		color: var(--color-text-secondary);
		font-style: italic;
		margin: 0;
	}

	.current-week-meals {
		margin-bottom: var(--space-6);
	}
	.current-week-meals h2 {
		margin-bottom: var(--space-3);
	}

	.current-week-meal-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.current-week-meal {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}
	.current-week-meal:hover {
		box-shadow: var(--shadow-md);
	}

	.current-week-meal__link {
		display: flex;
		gap: var(--space-3);
		text-decoration: none;
		color: inherit;
		padding: var(--space-3);
		align-items: center;
	}
	.current-week-meal__link:hover {
		background: var(--color-surface-2);
	}

	.current-week-meal__img {
		width: 56px;
		height: 56px;
		border-radius: var(--radius-sm);
		object-fit: cover;
		flex-shrink: 0;
	}

	.current-week-meal__info {
		display: flex;
		flex-direction: column;
		gap: var(--space-0-5);
		min-width: 0;
	}

	.current-week-meal__name {
		font-size: var(--text-base);
		font-weight: var(--weight-semibold);
	}

	.current-week-meal__ings {
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.current-week-summary {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		padding: var(--space-6);
	}
	.current-week-summary h2 {
		margin-top: 0;
	}

	.summary-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.summary-item {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		flex-wrap: wrap;
	}

	.summary-item__name {
		font-weight: var(--weight-medium);
		flex: 1;
	}

	.summary-item__num {
		font-weight: var(--weight-semibold);
		color: var(--color-primary);
	}

	.summary-item__text {
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}

</style>
