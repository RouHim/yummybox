<script lang="ts">
	import { listPlansForYear, getPlan, createPlan, updatePlan, deletePlan, listMeals } from '$lib/api';
	import type { Plan, PlanSummaryItem, Meal } from '$lib/types';
	import { t } from '$lib/i18n';
	import { weekOfDate, mondaySundayOf, weeksInYear, isPastWeek } from '$lib/week';
	import Icon from '$lib/Icon.svelte';

	let year = $state(new Date().getFullYear());
	let plans = $state<PlanSummaryItem[]>([]);
	let selectedWeek = $state<number | null>(null);
	let selectedPlan = $state<Plan | null>(null);
	let loading = $state(false);
	let planError = $state<string | null>(null);
	let mealCount = $state(3);

	// Meal picker state
	let mealPickerOpen = $state(false);
	let pickerSearch = $state('');
	let pickerResults = $state<Meal[]>([]);

	// Current week info
	let currentWeekInfo = $state(weekOfDate(new Date()));
	const totalWeeks = $derived(weeksInYear(year));

	$effect(() => {
		loadPlans();
	});

	$effect(() => {
		if (selectedWeek !== null) {
			loadPlan();
		}
	});

	async function loadPlans() {
		try {
			plans = await listPlansForYear(year);
		} catch {
			plans = [];
		}
	}

	async function loadPlan() {
		if (selectedWeek === null) return;
		loading = true;
		planError = null;
		try {
			selectedPlan = await getPlan(year, selectedWeek);
		} catch (err) {
			selectedPlan = null;
			if (err instanceof Error && err.message !== '__REQUEST_FAILED__') {
				planError = err.message;
			}
		} finally {
			loading = false;
		}
	}

	async function onGenerate() {
		planError = null;
		try {
			selectedPlan = await createPlan({ year, week_number: selectedWeek!, meal_count: mealCount });
			await loadPlans();
		} catch (err) {
			planError = err instanceof Error ? err.message : String(err);
		}
	}

	async function onDeletePlan() {
		if (selectedWeek === null) return;
		if (!confirm('Delete this plan?')) return;
		try {
			await deletePlan(year, selectedWeek);
			selectedPlan = null;
			await loadPlans();
		} catch (err) {
			planError = err instanceof Error ? err.message : String(err);
		}
	}

	async function onRemoveMeal(mealId: number) {
		if (!selectedPlan) return;
		const mealIds = selectedPlan.meals.map(m => m.id).filter(id => id !== mealId);
		try {
			selectedPlan = await updatePlan(year, selectedWeek!, { meal_ids: mealIds });
			await loadPlans();
		} catch (err) {
			planError = err instanceof Error ? err.message : String(err);
		}
	}

	async function openMealPicker() {
		mealPickerOpen = true;
		pickerSearch = '';
		await searchMeals('');
	}

	async function searchMeals(query: string) {
		try {
			pickerResults = await listMeals(query || undefined);
		} catch {
			pickerResults = [];
		}
	}

	async function onAddMeal(mealId: number) {
		if (!selectedPlan) return;
		const existing = selectedPlan.meals.map(m => m.id);
		if (existing.includes(mealId)) return; // already in plan
		try {
			selectedPlan = await updatePlan(year, selectedWeek!, { meal_ids: [...existing, mealId] });
			await loadPlans();
		} catch (err) {
			planError = err instanceof Error ? err.message : String(err);
		}
	}

	function formatDateRange(week: number): string {
		const { monday, sunday } = mondaySundayOf(year, week);
		const opts: Intl.DateTimeFormatOptions = { month: 'short', day: 'numeric' };
		return `${monday.toLocaleDateString(undefined, opts)} – ${sunday.toLocaleDateString(undefined, opts)}`;
	}

	function prevYear() { year--; selectedWeek = null; selectedPlan = null; }
	function nextYear() { year++; selectedWeek = null; selectedPlan = null; }
</script>

<main>
	<header class="planner-header">
		<div>
			<h1>{t('plannerTitle')}</h1>
			<p class="planner-header__subtitle">{t('plannerSubtitle')}</p>
		</div>
		<a href="/" class="nav-link">{t('navMeals')}</a>
	</header>

	<!-- Year navigation -->
	<div class="year-nav">
		<button class="btn btn--ghost btn--icon" onclick={prevYear} aria-label={t('plannerYearPrev')}>
			<Icon name="chevron-left" size={20} />
		</button>
		<span class="year-nav__label">{year}</span>
		<button class="btn btn--ghost btn--icon" onclick={nextYear} aria-label={t('plannerYearNext')}>
			<Icon name="chevron-right" size={20} />
		</button>
	</div>

	<!-- Week grid -->
	<div class="week-grid">
		{#each Array.from({ length: totalWeeks }, (_, i) => i + 1) as week}
			{@const weekPlan = plans.find(p => p.week_number === week)}
			{@const isCurrent = year === currentWeekInfo.year && week === currentWeekInfo.week}
			{@const isPast = isPastWeek(year, week, currentWeekInfo)}
			<button
				class="week-cell"
				class:week-cell--past={isPast}
				class:week-cell--current={isCurrent}
				class:week-cell--active={selectedWeek === week}
				class:week-cell--has-plan={!!weekPlan}
				onclick={() => { selectedWeek = week; mealCount = 3; }}
				aria-label="Week {week}: {formatDateRange(week)}"
			>
				<span class="week-cell__num">{week}</span>
				<span class="week-cell__dates">{formatDateRange(week)}</span>
				{#if weekPlan}
					<span class="week-cell__badge" title="{t('plannerHasPlan')} ({weekPlan.meal_count})">
						<Icon name="calendar" size={12} />
					</span>
				{/if}
			</button>
		{/each}
	</div>

	<!-- Plan detail panel -->
	{#if selectedWeek !== null}
		<section class="plan-detail">
			<h2>{t('plannerOpen')}: Week {selectedWeek}</h2>

			{#if loading}
				<p>Loading...</p>
			{:else if selectedPlan}
				<!-- Existing plan -->
				<div class="plan-meals">
					<h3>{selectedPlan.meals.length} meal{selectedPlan.meals.length !== 1 ? 's' : ''}</h3>
					{#if selectedPlan.meals.length === 0}
						<p class="plan-empty-msg">{t('plannerNoMeals')}</p>
					{:else}
						<ul class="plan-meal-list">
							{#each selectedPlan.meals as meal (meal.id)}
								<li class="plan-meal-item">
									<div>
										<strong>{meal.name}</strong>
										<span class="plan-meal-item__ings">
											{meal.ingredients.map(i => i.quantity ? `${i.name} (${i.quantity})` : i.name).join(', ')}
										</span>
									</div>
									<button class="btn btn--ghost btn--icon" onclick={() => onRemoveMeal(meal.id)}
										aria-label="{t('plannerRemove')} {meal.name}">
										<Icon name="trash" size={14} />
									</button>
								</li>
							{/each}
						</ul>
					{/if}

					<!-- Add meal button -->
					<div class="plan-add-row">
						<button class="btn btn--ghost" onclick={openMealPicker}>
							<Icon name="plus" size={14} /> {t('plannerAddMeal')}
						</button>
					</div>
				</div>

				<!-- Ingredient summary -->
				{#if selectedPlan.ingredient_summary.length > 0}
					<div class="plan-summary">
						<h3>{t('plannerIngredientSummary')}</h3>
						<ul class="summary-list">
							{#each selectedPlan.ingredient_summary as entry (entry.name)}
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
					</div>
				{/if}

				<div class="plan-actions">
					<button class="btn btn--danger-ghost" onclick={onDeletePlan}>
						{t('buttonDelete')}
					</button>
				</div>
			{:else}
				<!-- No plan — show generate form -->
				<div class="plan-generate">
					<p class="plan-empty-msg">{t('plannerEmptyState')}</p>
					<label class="field">
						<span class="field__label">{t('plannerCount')}</span>
						<input type="number" bind:value={mealCount} min={1} max={20} class="plan-count-input" />
					</label>
					<button class="btn btn--primary" onclick={onGenerate} aria-label={t('plannerGenerateAria')}>
						{t('plannerGenerate')}
					</button>
				</div>
			{/if}

			{#if planError}
				<p class="form-error" role="alert">
					<Icon name="alert" size={18} />
					<span>{planError}</span>
				</p>
			{/if}
		</section>
	{/if}

	<!-- Meal picker overlay -->
	{#if mealPickerOpen}
		<div class="meal-picker-overlay" onclick={() => mealPickerOpen = false} onkeydown={(e) => { if (e.key === 'Escape') mealPickerOpen = false; }} role="dialog" aria-label={t('plannerPickMeals')} tabindex="0">
			<div class="meal-picker" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
				<div class="meal-picker__header">
					<h3>{t('plannerPickMeals')}</h3>
					<button class="btn btn--ghost btn--icon" onclick={() => mealPickerOpen = false}>
						<Icon name="alert" size={18} />
					</button>
				</div>
				<input
					type="search"
					class="meal-picker__search"
					bind:value={pickerSearch}
					placeholder={t('searchPlaceholder')}
					oninput={(e) => searchMeals((e.target as HTMLInputElement).value)}
				/>
				<ul class="meal-picker__results">
					{#each pickerResults as meal (meal.id)}
						<li class="meal-picker__item">
							<span>{meal.name}</span>
							<button class="btn btn--primary btn--sm" onclick={() => onAddMeal(meal.id)}>
								{t('plannerAddMeal')}
							</button>
						</li>
					{/each}
					{#if pickerResults.length === 0}
						<li class="meal-picker__empty">
							{pickerSearch ? t('noResults', { search: pickerSearch }) : t('plannerPickMealsHelp')}
						</li>
					{/if}
				</ul>
			</div>
		</div>
	{/if}
</main>

<style>
	.planner-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: var(--space-4);
	}
	.planner-header__subtitle {
		margin: 0;
		color: var(--color-text-secondary);
		font-size: var(--text-base);
	}
	.nav-link {
		color: var(--color-primary);
		text-decoration: none;
		font-weight: var(--weight-medium);
		font-size: var(--text-sm);
	}
	.nav-link:hover { text-decoration: underline; }

	.year-nav {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-3);
		margin-bottom: var(--space-4);
	}
	.year-nav__label {
		font-size: var(--text-xl);
		font-weight: var(--weight-semibold);
		min-width: 5ch;
		text-align: center;
	}

	.week-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
		gap: var(--space-2);
		margin-bottom: var(--space-6);
	}

	.week-cell {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-1);
		padding: var(--space-3) var(--space-2);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		cursor: pointer;
		transition: background var(--transition-fast), border-color var(--transition-fast);
		position: relative;
	}
	.week-cell:hover {
		border-color: var(--color-primary);
	}
	.week-cell--current {
		border-color: var(--color-primary);
		border-width: 2px;
	}
	.week-cell--active {
		background: var(--color-primary-soft);
		border-color: var(--color-primary);
	}
	.week-cell--has-plan {
		background: var(--color-surface-2);
	}
	.week-cell--past {
		opacity: 0.5;
	}
	.week-cell--past .week-cell__num,
	.week-cell--past .week-cell__dates {
		color: var(--color-text-muted);
	}
	.week-cell--past.week-cell--active {
		opacity: 0.7;
	}
	.week-cell__num {
		font-weight: var(--weight-bold);
		font-size: var(--text-lg);
	}
	.week-cell__dates {
		font-size: var(--text-xs);
		color: var(--color-text-secondary);
	}
	.week-cell__badge {
		position: absolute;
		top: 4px;
		right: 4px;
		color: var(--color-primary);
	}

	.plan-detail {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		padding: var(--space-6);
		background: var(--color-surface);
	}
	.plan-detail h2 {
		margin-top: 0;
	}
	.plan-meals { margin-bottom: var(--space-4); }
	.plan-empty-msg {
		color: var(--color-text-secondary);
	}
	.plan-meal-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.plan-meal-item {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: var(--space-2) var(--space-3);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
	}
	.plan-meal-item__ings {
		display: block;
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
	.plan-add-row { margin-top: var(--space-3); }

	.summary-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.summary-item {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		padding: var(--space-1) 0;
		border-bottom: 1px solid var(--color-border);
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
	.plan-actions { margin-top: var(--space-4); }

	.plan-generate {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		align-items: flex-start;
	}
	.plan-count-input {
		max-width: 80px;
	}
	.form-error {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-top: var(--space-3);
		padding: var(--space-3);
		background: var(--color-danger-soft);
		border-radius: var(--radius-md);
		color: var(--color-danger);
		font-size: var(--text-sm);
	}

	/* Meal picker overlay */
	.meal-picker-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.3);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 100;
	}
	.meal-picker {
		background: var(--color-surface);
		border-radius: var(--radius-lg);
		padding: var(--space-6);
		max-width: 480px;
		width: 90%;
		max-height: 80vh;
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.meal-picker__header {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}
	.meal-picker__header h3 { margin: 0; }
	.meal-picker__search {
		width: 100%;
	}
	.meal-picker__results {
		list-style: none;
		padding: 0;
		margin: 0;
		overflow-y: auto;
		max-height: 400px;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.meal-picker__item {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: var(--space-2) var(--space-3);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
	}
	.meal-picker__empty {
		color: var(--color-text-secondary);
		text-align: center;
		padding: var(--space-8);
	}
	.btn--sm {
		padding: var(--space-1) var(--space-3);
		font-size: var(--text-sm);
	}
	.btn--icon {
		padding: var(--space-1);
		min-width: 36px;
		min-height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
	}
</style>
