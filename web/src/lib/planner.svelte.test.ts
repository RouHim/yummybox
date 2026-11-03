import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('./i18n', () => ({ t: (key: string) => key }));
vi.mock('./api', () => {
	class ApiError extends Error {
		code: string | null;
		status: number;
		constructor(message: string, code: string | null = null, status = 0) {
			super(message);
			this.name = 'ApiError';
			this.code = code;
			this.status = status;
		}
	}
	return {
		ApiError,
		getPlan: vi.fn(),
		listPlansForYear: vi.fn(),
		createPlan: vi.fn(),
		updatePlan: vi.fn(),
		deletePlan: vi.fn(),
	};
});

import { ApiError, deletePlan, getPlan, listPlansForYear, updatePlan } from './api';
import { PlannerState } from './planner.svelte';
import type { Plan, PlanSummaryItem } from './types';

const listPlansForYearMock = vi.mocked(listPlansForYear);
const getPlanMock = vi.mocked(getPlan);
const updatePlanMock = vi.mocked(updatePlan);
const deletePlanMock = vi.mocked(deletePlan);

function summary(year: number, week: number): PlanSummaryItem {
	return { year, week_number: week, id: year * 100 + week, meal_count: 3 };
}

function plan(mealIds: number[]): Plan {
	return {
		id: 1,
		year: 2026,
		week_number: 25,
		created_at: '2026-06-15T12:00:00Z',
		meals: mealIds.map((id) => ({
			id,
			name: `Meal ${id}`,
			ingredients: [],
			instructions: '',
			last_planned_at: null,
			created_at: '',
			updated_at: '',
			has_image: false,
			portions: null,
			source_url: null,
		})),
		ingredient_summary: [],
	};
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('PlannerState summaries', () => {
	it('fetches only the viewed year in a mid-year month', async () => {
		listPlansForYearMock.mockResolvedValueOnce([summary(2026, 25)]);
		const planner = new PlannerState();

		await planner.loadSummaries(2026, 5);

		expect(listPlansForYearMock).toHaveBeenCalledTimes(1);
		expect(listPlansForYearMock).toHaveBeenCalledWith(2026);
		expect(planner.plans).toEqual([summary(2026, 25)]);
	});

	it('merges the adjacent year for January and drops duplicate weeks', async () => {
		listPlansForYearMock
			.mockResolvedValueOnce([summary(2026, 1)])
			.mockResolvedValueOnce([summary(2025, 53), summary(2026, 1)]);
		const planner = new PlannerState();

		await planner.loadSummaries(2026, 0);

		expect(listPlansForYearMock).toHaveBeenNthCalledWith(1, 2026);
		expect(listPlansForYearMock).toHaveBeenNthCalledWith(2, 2025);
		expect(planner.plans.map((p) => `${p.year}-${p.week_number}`)).toEqual([
			'2026-1',
			'2025-53',
		]);
	});

	it('falls back to the viewed year when the adjacent fetch fails', async () => {
		listPlansForYearMock
			.mockResolvedValueOnce([summary(2026, 1)])
			.mockRejectedValueOnce(new Error('offline'));
		const planner = new PlannerState();

		await planner.loadSummaries(2026, 11);

		expect(planner.plans).toEqual([summary(2026, 1)]);
		expect(planner.planError).toBeNull();
	});

	it('keeps the previous plans and reports an error when the reload fails', async () => {
		listPlansForYearMock
			.mockResolvedValueOnce([summary(2026, 25)])
			.mockRejectedValueOnce(new ApiError('offline', 'REQUEST_FAILED', 0));
		const planner = new PlannerState();
		await planner.loadSummaries(2026, 5);

		await planner.loadSummaries(2026, 6);

		expect(planner.plans).toEqual([summary(2026, 25)]);
		expect(planner.planError).toBe('plannerError');
	});
});

describe('PlannerState selection', () => {
	it('resets the meal count when another week is selected', () => {
		const planner = new PlannerState();
		planner.mealCount = 7;

		planner.selectWeek(2026, 25);

		expect(planner.selectedWeekYear).toBe(2026);
		expect(planner.selectedWeek).toBe(25);
		expect(planner.mealCount).toBe(3);
	});

	it('does nothing when loading without a selected week', async () => {
		const planner = new PlannerState();

		await planner.loadSelected();

		expect(getPlanMock).not.toHaveBeenCalled();
		expect(planner.loading).toBe(false);
	});

	it('clears the plan and reports the actionable message when loading fails', async () => {
		getPlanMock.mockRejectedValueOnce(new ApiError('offline', 'REQUEST_FAILED', 0));
		const planner = new PlannerState();
		planner.selectWeek(2026, 25);

		await planner.loadSelected();

		expect(planner.selectedPlan).toBeNull();
		expect(planner.planError).toBe('plannerError');
		expect(planner.loading).toBe(false);
	});
});

describe('PlannerState mutations', () => {
	async function plannerWithPlan(): Promise<PlannerState> {
		listPlansForYearMock.mockResolvedValue([summary(2026, 25)]);
		getPlanMock.mockResolvedValue(plan([1]));
		const planner = new PlannerState();
		await planner.loadSummaries(2026, 5);
		planner.selectWeek(2026, 25);
		await planner.loadSelected();
		return planner;
	}

	it('adds a meal and refreshes the summaries', async () => {
		const planner = await plannerWithPlan();
		updatePlanMock.mockResolvedValueOnce(plan([1, 2]));

		await planner.addMeal(2);

		expect(updatePlanMock).toHaveBeenCalledWith(2026, 25, { meal_ids: [1, 2] });
		expect(planner.selectedPlan?.meals.map((m) => m.id)).toEqual([1, 2]);
		expect(listPlansForYearMock).toHaveBeenCalledTimes(2);
	});

	it('ignores a meal that is already planned', async () => {
		const planner = await plannerWithPlan();

		await planner.addMeal(1);

		expect(updatePlanMock).not.toHaveBeenCalled();
	});

	it('removes a meal and refreshes the summaries', async () => {
		const planner = await plannerWithPlan();
		updatePlanMock.mockResolvedValueOnce(plan([]));

		await planner.removeMeal(1);

		expect(updatePlanMock).toHaveBeenCalledWith(2026, 25, { meal_ids: [] });
		expect(listPlansForYearMock).toHaveBeenCalledTimes(2);
	});

	it('deletes the selected plan and refreshes the summaries', async () => {
		const planner = await plannerWithPlan();
		deletePlanMock.mockResolvedValueOnce(undefined);

		await planner.deleteSelected();

		expect(deletePlanMock).toHaveBeenCalledWith(2026, 25);
		expect(planner.selectedPlan).toBeNull();
		expect(listPlansForYearMock).toHaveBeenCalledTimes(2);
	});
});
