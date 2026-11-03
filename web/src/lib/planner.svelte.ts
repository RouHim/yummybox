import { ApiError, createPlan, deletePlan, getPlan, listPlansForYear, updatePlan } from './api';
import { t } from './i18n';
import type { Plan, PlanSummaryItem } from './types';

/**
 * Plan-editor state: the selected week, its plan, the year's summary list and
 * every mutation that refreshes them.
 *
 * The page owns calendar navigation and rendering; the invariant "a mutation
 * refreshes the summaries" and the network-error policy live here so they
 * cannot drift between the create, delete, add and remove paths.
 */
export class PlannerState {
	plans = $state<PlanSummaryItem[]>([]);
	selectedWeek = $state<number | null>(null);
	selectedWeekYear = $state<number | null>(null);
	selectedPlan = $state<Plan | null>(null);
	loading = $state(false);
	planError = $state<string | null>(null);
	mealCount = $state(3);

	// Last viewed month, so a mutation can refresh the summaries it invalidated.
	#viewYear: number | null = null;
	#viewMonth: number | null = null;

	selectWeek(year: number, week: number): void {
		this.selectedWeekYear = year;
		this.selectedWeek = week;
		this.mealCount = 3;
	}

	/** Load the plan of the selected week; no-op when nothing is selected. */
	async loadSelected(): Promise<void> {
		if (this.selectedWeek === null || this.selectedWeekYear === null) return;
		this.loading = true;
		this.planError = null;
		try {
			this.selectedPlan = await getPlan(this.selectedWeekYear, this.selectedWeek);
		} catch (err) {
			this.selectedPlan = null;
			this.planError = this.#message(err);
		} finally {
			this.loading = false;
		}
	}

	/**
	 * Load the summaries for the viewed month. Months near a year boundary
	 * also fetch the adjacent year, so border-week badges render.
	 */
	async loadSummaries(viewYear: number, viewMonth: number): Promise<void> {
		this.#viewYear = viewYear;
		this.#viewMonth = viewMonth;
		try {
			const primary = await listPlansForYear(viewYear);
			if (viewMonth !== 0 && viewMonth < 10) {
				this.plans = primary;
				this.planError = null;
				return;
			}
			const adjacentYear = viewMonth === 0 ? viewYear - 1 : viewYear + 1;
			const adjacent = await listPlansForYear(adjacentYear).catch(() => []);
			this.plans = mergeSummaries(primary, adjacent);
			this.planError = null;
		} catch {
			// Keep the previously loaded plans — a failed reload must not make
			// existing plans disappear.
			this.planError = t('plannerError');
		}
	}

	async createSelected(): Promise<void> {
		const year = this.selectedWeekYear;
		const week = this.selectedWeek;
		if (year === null || week === null) return;
		this.planError = null;
		try {
			this.selectedPlan = await createPlan({
				year,
				week_number: week,
				meal_count: this.mealCount,
			});
			await this.#refreshSummaries();
		} catch (err) {
			this.planError = this.#message(err);
		}
	}

	async deleteSelected(): Promise<void> {
		const year = this.selectedWeekYear;
		const week = this.selectedWeek;
		if (year === null || week === null) return;
		this.planError = null;
		try {
			await deletePlan(year, week);
			this.selectedPlan = null;
			await this.#refreshSummaries();
		} catch (err) {
			this.planError = this.#message(err);
		}
	}

	/** Add a meal unless it is already planned, then refresh the summaries. */
	async addMeal(mealId: number): Promise<void> {
		const plan = this.selectedPlan;
		if (!plan) return;
		const mealIds = plan.meals.map((meal) => meal.id);
		if (mealIds.includes(mealId)) return;
		await this.#updateMeals([...mealIds, mealId]);
	}

	async removeMeal(mealId: number): Promise<void> {
		const plan = this.selectedPlan;
		if (!plan) return;
		await this.#updateMeals(plan.meals.map((meal) => meal.id).filter((id) => id !== mealId));
	}

	async #updateMeals(mealIds: number[]): Promise<void> {
		const year = this.selectedWeekYear;
		const week = this.selectedWeek;
		if (year === null || week === null) return;
		this.planError = null;
		try {
			this.selectedPlan = await updatePlan(year, week, { meal_ids: mealIds });
			await this.#refreshSummaries();
		} catch (err) {
			this.planError = this.#message(err);
		}
	}

	async #refreshSummaries(): Promise<void> {
		if (this.#viewYear === null || this.#viewMonth === null) return;
		await this.loadSummaries(this.#viewYear, this.#viewMonth);
	}

	#message(err: unknown): string {
		if (err instanceof ApiError && err.code === 'REQUEST_FAILED') return t('plannerError');
		return err instanceof Error ? err.message : String(err);
	}
}

/** Merge two years' summaries, dropping the week that appears in both. */
function mergeSummaries(
	primary: PlanSummaryItem[],
	adjacent: PlanSummaryItem[]
): PlanSummaryItem[] {
	const seen = new Set<string>();
	const merged: PlanSummaryItem[] = [];
	for (const plan of [...primary, ...adjacent]) {
		const key = `${plan.year}-${plan.week_number}`;
		if (seen.has(key)) continue;
		seen.add(key);
		merged.push(plan);
	}
	return merged;
}
