<script lang="ts">
	import { listPlansForYear, getPlan, createPlan, updatePlan, deletePlan, listMeals, sendToBring, mealImageUrl } from '$lib/api';
	import type { Plan, PlanSummaryItem, Meal } from '$lib/types';
	import { t, formatDate } from '$lib/i18n';
	import { weekOfDate, mondaySundayOf, isPastWeek, monthGrid, type MonthCell } from '$lib/week';
	import Icon from '$lib/Icon.svelte';
	import { fly, fade, scale } from 'svelte/transition';
	import { flip } from 'svelte/animate';
	import { tierDuration } from '$lib/motion';
	import DeleteConfirmDialog from '$lib/DeleteConfirmDialog.svelte';
	import { page } from '$app/state';

	let viewYear = $state(new Date().getFullYear());
	let viewMonth = $state(new Date().getMonth()); // 0-indexed
	let plans = $state<PlanSummaryItem[]>([]);
	let selectedWeek = $state<number | null>(null);
	let selectedWeekYear = $state<number | null>(null);
	let selectedPlan = $state<Plan | null>(null);
	let loading = $state(false);
	let planError = $state<string | null>(null);
	let mealCount = $state(3);

	// Meal picker state
	let mealPickerOpen = $state(false);
	let planDeleteOpen = $state(false);
	let pickerSearch = $state('');
	let pickerResults = $state<Meal[]>([]);

	// Current week info
	let currentWeekInfo = $state(weekOfDate(new Date()));
	const focusCurrent = $derived(page.url.searchParams.get('focus') === 'current');

	// Calendar grid for the viewed month
	const grid = $derived(monthGrid(viewYear, viewMonth));

	// Group the 42 cells into 6 week rows of 7 days each.
	// Each row's identity is the ISO week of its Monday (first cell).
	const weekRows = $derived(
		Array.from({ length: 6 }, (_, row) => {
			const start = row * 7;
			const cells = grid.slice(start, start + 7);
			return { week: cells[0].week, cells };
		})
	);

	// Derived today string for highlighting today's cell in the grid
	const todayStr = $derived(new Date().toDateString());

	$effect(() => {
		loadPlans();
	});

	$effect(() => {
		if (selectedWeek !== null && selectedWeekYear !== null) {
			loadPlan();
		}
	});

	$effect(() => {
		if (focusCurrent && selectedWeek === null) {
			selectedWeek = currentWeekInfo.week;
			selectedWeekYear = currentWeekInfo.year;
			const { monday } = mondaySundayOf(currentWeekInfo.year, currentWeekInfo.week);
			viewYear = monday.getUTCFullYear();
			viewMonth = monday.getUTCMonth();
			mealCount = 3;
		}
	});

	async function loadPlans() {
		try {
			const primaryPlans = await listPlansForYear(viewYear);
			// For months near year boundaries, also fetch adjacent ISO year plans
			// so that border-week badges render correctly.
			if (viewMonth === 0 || viewMonth >= 10) {
				const adjacentYear = viewMonth === 0 ? viewYear - 1 : viewYear + 1;
				try {
					const adjacentPlans = await listPlansForYear(adjacentYear);
					const seen = new Set<string>();
					const merged: PlanSummaryItem[] = [];
					for (const p of [...primaryPlans, ...adjacentPlans]) {
						const key = `${p.year}-${p.week_number}`;
						if (!seen.has(key)) {
							seen.add(key);
							merged.push(p);
						}
					}
					plans = merged;
					return;
				} catch {
					// Fall through to primary only
				}
			}
			plans = primaryPlans;
		} catch {
			plans = [];
		}
	}

	async function loadPlan() {
		if (selectedWeek === null || selectedWeekYear === null) return;
		loading = true;
		planError = null;
		try {
			selectedPlan = await getPlan(selectedWeekYear, selectedWeek);
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
			selectedPlan = await createPlan({ year: selectedWeekYear!, week_number: selectedWeek!, meal_count: mealCount });
			await loadPlans();
		} catch (err) {
			planError = err instanceof Error ? err.message : String(err);
		}
	}

	async function onDeletePlan() {
		planDeleteOpen = true;
	}

	async function confirmDeletePlan() {
		planDeleteOpen = false;
		if (selectedWeek === null || selectedWeekYear === null) return;
		try {
			await deletePlan(selectedWeekYear, selectedWeek);
			selectedPlan = null;
			await loadPlans();
		} catch (err) {
			planError = err instanceof Error ? err.message : String(err);
		}
	}

	async function onRemoveMeal(mealId: number) {
		if (!selectedPlan || selectedWeek === null || selectedWeekYear === null) return;
		const mealIds = selectedPlan.meals.map(m => m.id).filter(id => id !== mealId);
		try {
			selectedPlan = await updatePlan(selectedWeekYear, selectedWeek, { meal_ids: mealIds });
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
		if (!selectedPlan || selectedWeek === null || selectedWeekYear === null) return;
		const existing = selectedPlan.meals.map(m => m.id);
		if (existing.includes(mealId)) return; // already in plan
		try {
			selectedPlan = await updatePlan(selectedWeekYear, selectedWeek, { meal_ids: [...existing, mealId] });
			await loadPlans();
		} catch (err) {
			planError = err instanceof Error ? err.message : String(err);
		}
	}

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

	/** Format the Mon–Sun date range for a given ISO week. */
	function formatDateRange(year: number, week: number): string {
		const { monday, sunday } = mondaySundayOf(year, week);
		const opts: Intl.DateTimeFormatOptions = { month: 'short', day: 'numeric' };
		return `${monday.toLocaleDateString(undefined, opts)} to ${sunday.toLocaleDateString(undefined, opts)}`;
	}

	/** Cell date as a local date string for "today" comparison. */
	function cellLocalDateStr(cell: MonthCell): string {
		return new Date(cell.date.getUTCFullYear(), cell.date.getUTCMonth(), cell.date.getUTCDate()).toDateString();
	}

	// Weekday header dates (Mon–Sun reference week: Jan 5–11 2026)
	const weekdayDates = Array.from({ length: 7 }, (_, i) => new Date(2026, 0, 5 + i));

	function prevMonth() {
		if (viewMonth === 0) {
			viewMonth = 11;
			viewYear--;
		} else {
			viewMonth--;
		}
	}

	function nextMonth() {
		if (viewMonth === 11) {
			viewMonth = 0;
			viewYear++;
		} else {
			viewMonth++;
		}
	}

	function goToday() {
		const now = new Date();
		viewYear = now.getFullYear();
		viewMonth = now.getMonth();
	}

	function onWeekClick(week: { year: number; week: number }) {
		selectedWeekYear = week.year;
		selectedWeek = week.week;
		mealCount = 3;
	}

	// Bring! integration state — per-ingredient send tracking by ingredient name
	let bringStates = $state<Record<string, { loading: boolean; error: string | null; success: boolean }>>({});

	function bringSpec(entry: { name: string; numeric_total: { value: number; unit: string | null } | null }): string | null {
		if (entry.numeric_total) {
			const { value, unit } = entry.numeric_total;
			return unit ? `${value} ${unit}` : `${value}`;
		}
		return null;
	}

	async function onBringSend(entry: { name: string; numeric_total: { value: number; unit: string | null } | null }) {
		const key = entry.name;
		bringStates[key] = { loading: true, error: null, success: false };
		try {
			await sendToBring(entry.name, bringSpec(entry));
			bringStates[key] = { loading: false, error: null, success: true };
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			bringStates[key] = { loading: false, error: msg, success: false };
		}
	}
</script>

<main>

	<!-- Month navigation -->
	<div class="cal-nav glass">
		<button class="btn btn--ghost btn--icon" onclick={prevMonth} aria-label={t('plannerMonthPrev')}>
			<Icon name="chevron-left" size={20} />
		</button>
		<span class="cal-nav__label">{formatDate(new Date(viewYear, viewMonth, 1), { month: 'long', year: 'numeric' })}</span>
		<button class="btn btn--ghost btn--icon" onclick={nextMonth} aria-label={t('plannerMonthNext')}>
			<Icon name="chevron-right" size={20} />
		</button>
		<button class="btn btn--ghost btn--icon" onclick={goToday} aria-label={t('plannerToday')}>
			<Icon name="calendar" size={20} />
		</button>
	</div>

	<!-- Month calendar -->
	<div class="cal-grid">
		<!-- Weekday headers -->
		<div class="cal-grid__header">
			{#each weekdayDates as d}
				<div class="cal-grid__dow" role="columnheader">
					{formatDate(d, { weekday: 'short' }).replace(/\.$/, '')}
				</div>
			{/each}
		</div>

		<!-- 6 week rows -->
		{#each weekRows as row}
			{@const weekPlan = plans.find(p => p.year === row.week.year && p.week_number === row.week.week)}
			{@const isCurrent = row.week.year === currentWeekInfo.year && row.week.week === currentWeekInfo.week}
			{@const isPast = isPastWeek(row.week.year, row.week.week, currentWeekInfo)}
			{@const isActive = selectedWeek === row.week.week && selectedWeekYear === row.week.year}
			<button
				class="week-cell"
				class:week-cell--past={isPast}
				class:week-cell--current={isCurrent}
				class:week-cell--active={isActive}
				class:week-cell--has-plan={!!weekPlan}
				onclick={() => onWeekClick(row.week)}
				aria-label={t('plannerWeekAria', { week: String(row.week.week), range: formatDateRange(row.week.year, row.week.week) })}
			>
				{#each row.cells as cell}
					<span
						class="cal-day"
						class:cal-day--out={!cell.inMonth}
						class:cal-day--today={cellLocalDateStr(cell) === todayStr}
					>
						{cell.date.getUTCDate()}
					</span>
				{/each}
				{#if weekPlan}
					<span class="week-cell__badge" title={t('plannerHasPlan')}></span>
				{/if}
			</button>
		{/each}
	</div>

	<!-- Plan detail panel -->
	{#if selectedWeek !== null}
		<section class="plan-detail glass" in:fly={{ y: 8, duration: tierDuration(250) }}>
			<header class="plan-detail__header">
				<h2>{t('plannerOpen')}: Week {selectedWeek}</h2>
				{#if selectedWeekYear !== null}
					<p class="plan-detail__date-range">
						{formatDateRange(selectedWeekYear, selectedWeek)}
					</p>
				{/if}
			</header>

			{#if loading}
				<p>Loading...</p>
			{:else if selectedPlan}
				<!-- Existing plan -->
				<div class="plan-meals">
					{#if selectedPlan.meals.length === 0}
						<p class="plan-empty-msg">{t('plannerNoMeals')}</p>
					{:else}
						<div class="plan-meal-grid">
							{#each selectedPlan.meals as meal (meal.id)}
								<article class="plan-meal-card" animate:flip={{ duration: tierDuration(200) }}>
									<a href="/meals/{meal.id}" class="plan-meal-card__link" aria-label={t('mealCardCookAria', { name: meal.name })}>
										<div class="plan-meal-card__media">
											{#if meal.has_image}
												<img src={mealImageUrl(meal.id)} alt={meal.name} class="plan-meal-card__img" loading="lazy" />
											{:else}
												<div class="plan-meal-card__placeholder" aria-hidden="true">
													<Icon name="utensils" size={36} />
												</div>
											{/if}
										</div>
										<div class="plan-meal-card__body">
											<h3 class="plan-meal-card__name">{meal.name}</h3>
											<span class="plan-meal-card__chip">
												{meal.ingredients.length === 1
													? t('ingredientCountOne')
													: t('ingredientCount', { count: String(meal.ingredients.length) })}
											</span>
										</div>
									</a>
									<button
										class="plan-meal-card__remove"
										onclick={() => onRemoveMeal(meal.id)}
										aria-label="{t('plannerRemove')} {meal.name}"
										title="{t('plannerRemove')} {meal.name}"
									>
										<Icon name="trash-2" size={14} />
									</button>
								</article>
							{/each}

							<button class="plan-meal-card plan-meal-card--add" onclick={openMealPicker}>
								<div class="plan-meal-card__add-icon">
									<Icon name="plus" size={28} />
								</div>
								<span class="plan-meal-card__add-label">{t('plannerAddMeal')}</span>
							</button>
						</div>
					{/if}
				</div>

				<!-- Ingredient summary -->
				{#if selectedPlan.ingredient_summary.length > 0}
					<div class="plan-summary">
						<h3>{t('plannerIngredientSummary')}</h3>
						<div class="summary-grid">
							{#each selectedPlan.ingredient_summary as entry (entry.name)}
								{@const bs = bringStates[entry.name] ?? { loading: false, error: null, success: false }}
								<div class="summary-card" class:summary-card--ok={bs.success}>
									<div class="summary-card__main">
										<span class="summary-card__name">{entry.name}</span>
										{#if entry.numeric_total}
											<span class="summary-card__num">
												{entry.numeric_total.value}{#if entry.numeric_total.unit} {entry.numeric_total.unit}{/if}
											</span>
										{/if}
										{#each entry.non_numeric as qty}
											<span class="summary-card__text">{qty}</span>
										{/each}
									</div>
									<button
										class="bring-btn"
										class:bring-btn--loading={bs.loading}
										class:bring-btn--success={bs.success}
										class:bring-btn--error={bs.error !== null}
										onclick={() => onBringSend(entry)}
										disabled={bs.loading || bs.success}
										aria-label={bs.loading ? t('bringSending') : bs.success ? t('bringSent') : t('bringSend')}
										title={bs.loading ? t('bringSending') : bs.success ? t('bringSent') : t('bringSend')}
									>
										{#if bs.loading}
											<Icon name="loader-circle" size={16} spin={true} />
										{:else if bs.success}
											<Icon name="check" size={16} />
										{:else}
											<Icon name="shopping-bag" size={16} />
										{/if}
									</button>
									{#if bs.error}
										<p class="summary-card__error" role="alert">
											<Icon name="circle-alert" size={12} />
											{bs.error}
										</p>
									{/if}
								</div>
							{/each}
						</div>
					</div>
				{/if}

				<div class="plan-actions">
					<button class="btn btn--danger-ghost" onclick={onDeletePlan}>
						<Icon name="trash-2" size={16} />
						{t('plannerDeletePlan')}
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
					<Icon name="circle-alert" size={18} />
					<span>{planError}</span>
				</p>
			{/if}
		</section>
	{/if}

	<!-- Meal picker overlay -->
	{#if mealPickerOpen}
		<div class="meal-picker-overlay glass--strong" transition:fade={{ duration: tierDuration(200) }} onclick={() => mealPickerOpen = false} onkeydown={(e) => { if (e.key === 'Escape') mealPickerOpen = false; }} role="dialog" aria-label={t('plannerPickMeals')} tabindex="-1">
			<div class="meal-picker" use:focusTrap in:scale={{ duration: tierDuration(250), start: 0.95 }} onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
				<div class="meal-picker__header">
					<h3>{t('plannerPickMeals')}</h3>
					<button class="btn btn--ghost btn--icon" onclick={() => mealPickerOpen = false} aria-label={t('plannerClose')}>
						<Icon name="x" size={18} />
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
	<DeleteConfirmDialog
		open={planDeleteOpen}
		title={t('plannerDeletePlan')}
		message={t('confirmDeletePlan')}
		confirmLabel={t('plannerDeletePlan')}
		cancelLabel={t('buttonCancel')}
		onconfirm={confirmDeletePlan}
		oncancel={() => (planDeleteOpen = false)}
	/>
</main>

<style>
	/* ---- Calendar Navigation ---- */
	.cal-nav {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-3);
		margin-bottom: var(--space-4);
		position: sticky;
		top: var(--app-bar-h);
		z-index: 15;
		padding: var(--space-2) var(--space-4);
		border-radius: var(--radius-md);
	}
	.cal-nav__label {
		font-size: var(--text-xl);
		font-weight: var(--weight-semibold);
		min-width: 16ch;
		text-align: center;
	}

	/* ---- Calendar Grid ---- */
	.cal-grid {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin-bottom: var(--space-6);
	}
	.cal-grid__dow {
		text-align: center;
		font-size: var(--text-xs);
		color: var(--color-text-muted);
		padding: var(--space-1) 0;
		text-transform: none;
	}

	.cal-grid__header {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		gap: var(--space-1);
	}

	/* ---- Week Row (the clickable .week-cell) ---- */
	.week-cell {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		gap: var(--space-1);
		width: 100%;
		padding: 0;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		cursor: pointer;
		transition: background var(--motion-morph), border-color var(--motion-morph), transform var(--motion-morph);
		transform: scale(1);
		position: relative;
	}
	.week-cell:active {
		transform: var(--motion-scale-press);
	}
	.week-cell:hover {
		border-color: var(--color-primary);
	}
	.week-cell--current {
		border-color: var(--color-border);
		border-bottom: 3px solid var(--color-primary);
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
	.week-cell--past.week-cell--active {
		opacity: 0.7;
	}

	/* ---- Day Chip ---- */
	.cal-day {
		padding: var(--space-2) var(--space-1);
		text-align: center;
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
	.cal-day--out {
		opacity: 0.4;
	}
	.cal-day--today {
		font-weight: var(--weight-bold);
		color: var(--color-primary);
	}

	/* ---- Plan Badge ---- */
	.week-cell__badge {
		position: absolute;
		top: 6px;
		right: 6px;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--color-primary);
	}

	/* ---- Plan Detail Panel ---- */
	.plan-detail {
		border: 1px solid var(--glass-border);
		border-radius: var(--radius-lg);
		padding: var(--space-6);
	}
	.plan-detail__header {
		margin-bottom: var(--space-5);
	}
	.plan-detail__header h2 {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-xl);
		font-weight: var(--weight-semibold);
	}
	.plan-detail__date-range {
		margin: var(--space-1) 0 0;
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}

	/* ---- Meal card grid ---- */
	.plan-meals { margin-bottom: var(--space-6); }
	.plan-empty-msg {
		color: var(--color-text-secondary);
		padding: var(--space-4) 0;
	}
	.plan-meal-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: var(--space-3);
	}
	.plan-meal-card {
		position: relative;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		overflow: hidden;
		transition: border-color var(--transition-base), box-shadow var(--transition-base), transform var(--transition-base);
	}
	.plan-meal-card:hover {
		border-color: var(--color-border-strong);
		box-shadow: var(--shadow-md);
		transform: translateY(-2px);
	}
	.plan-meal-card__link {
		display: block;
		text-decoration: none;
		color: inherit;
	}
	.plan-meal-card__media {
		aspect-ratio: 16 / 9;
		background: var(--color-surface-2);
		overflow: hidden;
	}
	.plan-meal-card__img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.plan-meal-card__placeholder {
		width: 100%;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-text-muted);
		background: var(--color-primary-soft);
	}
	.plan-meal-card__body {
		padding: var(--space-2) var(--space-3);
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.plan-meal-card__name {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-base);
		font-weight: var(--weight-semibold);
		line-height: 1.3;
	}
	.plan-meal-card__chip {
		font-size: var(--text-xs);
		font-weight: var(--weight-medium);
		color: var(--color-text-muted);
	}
	.plan-meal-card__remove {
		position: absolute;
		top: var(--space-1);
		right: var(--space-1);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		padding: 0;
		border: 0;
		border-radius: var(--radius-full);
		background: var(--glass-bg-strong);
		color: var(--color-text-secondary);
		cursor: pointer;
		opacity: 0;
		pointer-events: none;
		transition: opacity var(--transition-fast), color var(--transition-fast), background var(--transition-fast);
		backdrop-filter: blur(var(--glass-blur-low));
		-webkit-backdrop-filter: blur(var(--glass-blur-low));
	}
	.plan-meal-card:hover .plan-meal-card__remove,
	.plan-meal-card:focus-within .plan-meal-card__remove {
		opacity: 1;
		pointer-events: auto;
	}
	.plan-meal-card__remove:hover {
		color: var(--color-danger);
		background: var(--color-danger-soft);
	}
	.plan-meal-card__remove:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
		opacity: 1;
		pointer-events: auto;
	}
	@media (hover: none) {
		.plan-meal-card__remove {
			opacity: 1;
			pointer-events: auto;
		}
	}

	/* ---- Add-meal tile ---- */
	.plan-meal-card--add {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		padding: var(--space-4);
		border-style: dashed;
		border-color: var(--color-border-strong);
		background: transparent;
		color: var(--color-text-secondary);
		cursor: pointer;
	}
	.plan-meal-card--add:hover {
		border-color: var(--color-primary);
		color: var(--color-primary);
		background: var(--color-primary-soft);
		transform: translateY(-2px);
	}
	.plan-meal-card__add-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		color: inherit;
	}
	.plan-meal-card__add-label {
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
	}

	/* ---- Ingredient summary grid ---- */
	.plan-summary {
		margin-bottom: var(--space-4);
	}
	.plan-summary h3 {
		margin: 0 0 var(--space-3);
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--weight-semibold);
	}
	.summary-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: var(--space-2);
	}
	.summary-card {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		transition: border-color var(--transition-fast);
	}
	.summary-card--ok {
		border-color: var(--color-success);
		background: var(--color-success-bg);
	}
	.summary-card__main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-1);
	}
	.summary-card__name {
		font-weight: var(--weight-medium);
		font-size: var(--text-sm);
	}
	.summary-card__num {
		font-weight: var(--weight-semibold);
		color: var(--color-primary);
		font-size: var(--text-sm);
	}
	.summary-card__text {
		font-size: var(--text-xs);
		color: var(--color-text-secondary);
	}
	.summary-card__error {
		display: flex;
		align-items: center;
		gap: var(--space-1);
		margin: var(--space-1) 0 0;
		color: var(--color-danger);
		font-size: var(--text-xs);
	}

	/* ---- Actions ---- */
	.plan-actions {
		margin-top: var(--space-4);
		display: flex;
		justify-content: flex-end;
	}

	/* ---- Generate form ---- */
	.plan-generate {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		align-items: flex-start;
		padding: var(--space-2) 0;
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

	/* Meal picker overlay (unchanged) */
	.meal-picker-overlay {
		position: fixed;
		inset: 0;
		background: var(--glass-scrim-dark);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 100;
		padding: var(--space-4);
	}
	.meal-picker {
		background: transparent;
		border: 1px solid var(--glass-border);
		border-radius: var(--radius-lg);
		padding: var(--space-6);
		max-width: 480px;
		width: 100%;
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

	/* ---- Mobile ---- */
	@media (max-width: 767px) {
		.cal-day {
			font-size: var(--text-xs);
			padding: var(--space-1) var(--space-0-5);
		}
		.cal-nav__label {
			min-width: 12ch;
		}
		.plan-meal-grid {
			grid-template-columns: repeat(2, 1fr);
			gap: var(--space-2);
		}
		.summary-grid {
			grid-template-columns: 1fr;
		}
	}
	/* ---- Bring! button ---- */
	.bring-btn {
		flex-shrink: 0;
		margin-left: auto;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		padding: 0;
		border: 1px solid var(--glass-border);
		background: var(--glass-bg);
		color: var(--color-text-secondary);
		border-radius: var(--radius-full);
		cursor: pointer;
		transition: color var(--transition-fast), border-color var(--transition-fast), background var(--transition-fast);
	}
	.bring-btn:hover {
		color: var(--color-primary);
		border-color: var(--color-border-strong);
		background: var(--glass-bg-strong);
	}
	.bring-btn:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}
	.bring-btn:disabled {
		cursor: wait;
		opacity: 0.6;
	}
	.bring-btn--loading {
		opacity: 0.6;
		cursor: wait;
	}
	.bring-btn--success {
		color: var(--color-success);
	}
	.bring-btn--error {
		color: var(--color-danger);
	}
	@media (prefers-reduced-motion: reduce) {
		.bring-btn {
			transition: none;
		}
	}
</style>
