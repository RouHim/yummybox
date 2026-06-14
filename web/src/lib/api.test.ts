import { describe, it, expect, vi, beforeEach } from 'vitest';
import { listMeals, createMeal, updateMeal, deleteMeal, listPlansForYear, getPlan, createPlan, updatePlan, deletePlan } from './api';
import type { Meal, MealPayload, Plan, PlanSummaryItem, NewPlanRequest, PlanPatch } from './types';

const mockFetch = vi.fn();
globalThis.fetch = mockFetch;

beforeEach(() => {
	mockFetch.mockReset();
});

function mockResponse(status: number, body?: unknown) {
	mockFetch.mockResolvedValueOnce({
		ok: status >= 200 && status < 300,
		status,
		json: async () => body,
		text: async () => ''
	});
}

// ---------------------------------------------------------------------------
// Meal API
// ---------------------------------------------------------------------------

describe('listMeals', () => {
	it('calls /api/meals without search param', async () => {
		mockResponse(200, []);
		await listMeals();
		expect(mockFetch).toHaveBeenCalledWith('/api/meals', undefined);
	});

	it('appends search query param when search is provided', async () => {
		mockResponse(200, []);
		await listMeals('chicken');
		expect(mockFetch).toHaveBeenCalledWith('/api/meals?search=chicken', undefined);
	});
});

describe('createMeal', () => {
	it('posts payload with structured ingredients to /api/meals', async () => {
		const payload: MealPayload = { name: 'Test', ingredients: [{ name: 'stuff', quantity: null }] };
		const mealResponse: Meal = { id: 1, name: 'Test', ingredients: [{ name: 'stuff', quantity: null }], last_planned_at: null, created_at: '', updated_at: '' };
		mockResponse(201, mealResponse);
		await createMeal(payload);
		expect(mockFetch).toHaveBeenCalledWith('/api/meals', {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(payload)
		});
	});
});

describe('updateMeal', () => {
	it('puts payload with structured ingredients to /api/meals/:id', async () => {
		const payload: MealPayload = { name: 'Updated', ingredients: [{ name: 'new', quantity: null }] };
		const mealResponse: Meal = { id: 3, name: 'Updated', ingredients: [{ name: 'new', quantity: null }], last_planned_at: null, created_at: '', updated_at: '' };
		mockResponse(200, mealResponse);
		await updateMeal(3, payload);
		expect(mockFetch).toHaveBeenCalledWith('/api/meals/3', {
			method: 'PUT',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(payload)
		});
	});
});

describe('deleteMeal', () => {
	it('deletes /api/meals/:id', async () => {
		mockResponse(204);
		await deleteMeal(3);
		expect(mockFetch).toHaveBeenCalledWith('/api/meals/3', {
			method: 'DELETE'
		});
	});
});

describe('error handling', () => {
	it('throws with server error message on 400', async () => {
		mockResponse(400, { error: 'name must not be empty' });
		await expect(createMeal({ name: '', ingredients: [{ name: 'x', quantity: null }] })).rejects.toThrow('name must not be empty');
	});
});

// ---------------------------------------------------------------------------
// Plan API
// ---------------------------------------------------------------------------

describe('listPlansForYear', () => {
	it('calls correct url and returns array', async () => {
		const items: PlanSummaryItem[] = [{ year: 2026, week_number: 1, id: 1, meal_count: 3 }];
		mockResponse(200, items);
		const result = await listPlansForYear(2026);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans?year=2026', undefined);
		expect(result).toEqual(items);
	});

	it('throws on 500 with server error message', async () => {
		mockResponse(500, { error: 'boom' });
		await expect(listPlansForYear(2026)).rejects.toThrow('boom');
	});
});

describe('getPlan', () => {
	it('calls correct url and returns plan', async () => {
		const plan: Plan = { id: 1, year: 2026, week_number: 1, created_at: '', meals: [], ingredient_summary: [] };
		mockResponse(200, plan);
		const result = await getPlan(2026, 1);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans?year=2026&week=1');
		expect(result).toEqual(plan);
	});

	it('given_array_response_then_throws', async () => {
		mockResponse(200, []);
		await expect(getPlan(2026, 1)).rejects.toThrow('expected plan, got array');
	});

	it('given_404_then_resolves_to_null', async () => {
		mockResponse(404, { error: 'not found' });
		await expect(getPlan(2026, 1)).resolves.toBeNull();
	});

	it('given_500_then_throws_with_server_message', async () => {
		mockResponse(500, { error: 'boom' });
		await expect(getPlan(2026, 1)).rejects.toThrow('boom');
	});
});

describe('createPlan', () => {
	it('posts to plans with json body and returns plan', async () => {
		const req: NewPlanRequest = { year: 2026, week_number: 1, meal_count: 3 };
		const plan: Plan = { id: 1, year: 2026, week_number: 1, created_at: '', meals: [], ingredient_summary: [] };
		mockResponse(201, plan);
		const result = await createPlan(req);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans', {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(req)
		});
		expect(result).toEqual(plan);
	});

	it('throws on 400 with server error message', async () => {
		mockResponse(400, { error: 'no meals exist' });
		await expect(createPlan({ year: 2026, week_number: 1, meal_count: 3 })).rejects.toThrow('no meals exist');
	});
});

describe('updatePlan', () => {
	it('puts to plans year week with json body and returns plan', async () => {
		const patch: PlanPatch = { meal_ids: [1, 2] };
		const plan: Plan = { id: 1, year: 2026, week_number: 1, created_at: '', meals: [], ingredient_summary: [] };
		mockResponse(200, plan);
		const result = await updatePlan(2026, 1, patch);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans/2026/1', {
			method: 'PUT',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(patch)
		});
		expect(result).toEqual(plan);
	});
});

describe('deletePlan', () => {
	it('deletes plans year week', async () => {
		mockResponse(204);
		await deletePlan(2026, 1);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans/2026/1', {
			method: 'DELETE'
		});
	});

	it('throws on 404 with server error message', async () => {
		mockResponse(404, { error: 'not found' });
		await expect(deletePlan(2026, 1)).rejects.toThrow('not found');
	});
});
