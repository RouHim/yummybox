import { describe, it, expect, vi, beforeEach } from 'vitest';
import { listMeals, getMeal, createMeal, updateMeal, deleteMeal, mealImageUrl, listPlansForYear, getPlan, createPlan, updatePlan, deletePlan, importFromUrl, importFromPaste } from './api';
import type { Meal, MealPayload, Plan, PlanSummaryItem, NewPlanRequest, PlanPatch } from './types';

const mockFetch = vi.fn();
globalThis.fetch = mockFetch;

beforeEach(() => {
	mockFetch.mockReset();
});

function mockResponse(status: number, body?: unknown) {
	const init: ResponseInit = { status };
	if (body !== undefined) {
		mockFetch.mockResolvedValueOnce({
			ok: status >= 200 && status < 300,
			status,
			json: async () => body,
		} satisfies Partial<Response>);
	} else {
		mockFetch.mockResolvedValueOnce({
			ok: status >= 200 && status < 300,
			status,
		} satisfies Partial<Response>);
	}
}

// ---------------------------------------------------------------------------
// Meal API
// ---------------------------------------------------------------------------

describe('listMeals', () => {
	it('calls /api/meals without search', async () => {
		mockResponse(200, []);
		await listMeals();
		expect(mockFetch).toHaveBeenCalledWith('/api/meals', undefined);
	});

	it('calls /api/meals?search=...', async () => {
		mockResponse(200, []);
		await listMeals('pizza');
		expect(mockFetch).toHaveBeenCalledWith('/api/meals?search=pizza', undefined);
	});
});

describe('getMeal', () => {
	it('calls /api/meals/:id', async () => {
		const meal: Meal = { id: 5, name: 'Pasta', ingredients: [], last_planned_at: null, created_at: '', updated_at: '', has_image: false, instructions: '' };
		mockResponse(200, meal);
		const result = await getMeal(5);
		expect(mockFetch).toHaveBeenCalledWith('/api/meals/5', undefined);
		expect(result).toEqual(meal);
	});

	it('throws on 404', async () => {
		mockResponse(404);
		await expect(getMeal(999)).rejects.toThrow();
	});
});

describe('createMeal', () => {
	it('sends multipart form with name and ingredients', async () => {
		const payload: MealPayload = { name: 'Test', ingredients: [{ name: 'stuff', quantity: null }] , instructions: '' };
		const mealResponse: Meal = { id: 1, name: 'Test', ingredients: [{ name: 'stuff', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: false, instructions: '' };
		mockResponse(201, mealResponse);
		await createMeal(payload);
		expect(mockFetch).toHaveBeenCalledTimes(1);
		const [url, opts] = mockFetch.mock.calls[0];
		expect(url).toBe('/api/meals');
		expect(opts.method).toBe('POST');
		expect(opts.body).toBeInstanceOf(FormData);
		const fd = opts.body as FormData;
		expect(fd.get('name')).toBe('Test');
		expect(fd.get('ingredients')).toBe(JSON.stringify(payload.ingredients));
		expect(fd.get('image')).toBeNull();
		// Browser sets multipart boundary — no explicit content-type header
		expect(opts.headers).toBeUndefined();
	});

	it('includes image file when provided', async () => {
		const payload: MealPayload = { name: 'Pizza', ingredients: [{ name: 'cheese', quantity: null }] , instructions: '' };
		const mealResponse: Meal = { id: 2, name: 'Pizza', ingredients: [{ name: 'cheese', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: true, instructions: '' };
		mockResponse(201, mealResponse);
		const file = new File([new Uint8Array([1, 2, 3])], 'photo.png', { type: 'image/png' });
		await createMeal(payload, file);
		const fd = mockFetch.mock.calls[0][1].body as FormData;
		expect(fd.get('image')).toBeInstanceOf(File);
		expect((fd.get('image') as File).name).toBe('photo.png');
	});

	it('handles null image gracefully', async () => {
		const payload: MealPayload = { name: 'X', ingredients: [{ name: 'y', quantity: null }] , instructions: '' };
		const mealResponse: Meal = { id: 3, name: 'X', ingredients: [{ name: 'y', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: false, instructions: '' };
		mockResponse(201, mealResponse);
		await createMeal(payload, null);
		const fd = mockFetch.mock.calls[0][1].body as FormData;
		expect(fd.get('image')).toBeNull();
	});
});

describe('updateMeal', () => {
	it('sends multipart form with name and ingredients', async () => {
		const payload: MealPayload = { name: 'Updated', ingredients: [{ name: 'new', quantity: null }] , instructions: '' };
		const mealResponse: Meal = { id: 3, name: 'Updated', ingredients: [{ name: 'new', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: false, instructions: '' };
		mockResponse(200, mealResponse);
		await updateMeal(3, payload);
		const [url, opts] = mockFetch.mock.calls[0];
		expect(url).toBe('/api/meals/3');
		expect(opts.method).toBe('PUT');
		const fd = opts.body as FormData;
		expect(fd.get('name')).toBe('Updated');
		expect(fd.get('ingredients')).toBe(JSON.stringify(payload.ingredients));
		expect(fd.get('image_action')).toBeNull();
	});

	it('sends image_action=remove when removing', async () => {
		const payload: MealPayload = { name: 'X', ingredients: [{ name: 'y', quantity: null }] , instructions: '' };
		const mealResponse: Meal = { id: 4, name: 'X', ingredients: [{ name: 'y', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: false, instructions: '' };
		mockResponse(200, mealResponse);
		await updateMeal(4, payload, { removeImage: true });
		const fd = mockFetch.mock.calls[0][1].body as FormData;
		expect(fd.get('image_action')).toBe('remove');
	});

	it('sends image file when replacing', async () => {
		const payload: MealPayload = { name: 'X', ingredients: [{ name: 'y', quantity: null }] , instructions: '' };
		const mealResponse: Meal = { id: 5, name: 'X', ingredients: [{ name: 'y', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: true, instructions: '' };
		mockResponse(200, mealResponse);
		const file = new File([new Uint8Array([4, 5, 6])], 'new.jpg', { type: 'image/jpeg' });
		await updateMeal(5, payload, { image: file });
		const fd = mockFetch.mock.calls[0][1].body as FormData;
		expect(fd.get('image')).toBeInstanceOf(File);
		expect((fd.get('image') as File).name).toBe('new.jpg');
	});
});

describe('deleteMeal', () => {
	it('deletes /api/meals/:id', async () => {
		mockResponse(204);
		await deleteMeal(7);
		expect(mockFetch).toHaveBeenCalledWith('/api/meals/7', { method: 'DELETE' });
	});
});

describe('mealImageUrl', () => {
	it('returns the correct image endpoint URL', () => {
		expect(mealImageUrl(42)).toBe('/api/meals/42/image');
	});
});

describe('error handling', () => {
	it('extracts server error message from JSON body', async () => {
		mockResponse(400, { error: 'name must not be empty' });
		await expect(listMeals()).rejects.toThrow('name must not be empty');
	});
});

// ---------------------------------------------------------------------------
// Plan API
// ---------------------------------------------------------------------------

describe('listPlansForYear', () => {
	it('calls /api/plans?year=...', async () => {
		mockResponse(200, []);
		await listPlansForYear(2026);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans?year=2026', undefined);
	});

	it('throws on non-array response', async () => {
		mockResponse(200, { not: 'array' });
		await expect(listPlansForYear(2026)).rejects.toThrow('expected array');
	});
});

describe('getPlan', () => {
	it('calls /api/plans?year=...&week=...', async () => {
		const plan: Plan = { id: 1, year: 2026, week_number: 1, created_at: '', meals: [], ingredient_summary: [] };
		mockResponse(200, plan);
		const result = await getPlan(2026, 1);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans?year=2026&week=1');
		expect(result).toEqual(plan);
	});

	it('returns null on 404', async () => {
		mockResponse(404);
		const result = await getPlan(2026, 53);
		expect(result).toBeNull();
	});
});

describe('createPlan', () => {
	it('posts JSON body', async () => {
		const payload: NewPlanRequest = { year: 2026, week_number: 1, meal_count: 3 };
		const plan: Plan = { id: 1, year: 2026, week_number: 1, created_at: '', meals: [], ingredient_summary: [] };
		mockResponse(201, plan);
		await createPlan(payload);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans', {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(payload)
		});
	});
});

describe('updatePlan', () => {
	it('puts JSON body', async () => {
		const payload: PlanPatch = { meal_ids: [1, 2] };
		const plan: Plan = { id: 1, year: 2026, week_number: 1, created_at: '', meals: [], ingredient_summary: [] };
		mockResponse(200, plan);
		await updatePlan(2026, 1, payload);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans/2026/1', {
			method: 'PUT',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(payload)
		});
	});
});

describe('deletePlan', () => {
	it('deletes /api/plans/:year/:week', async () => {
		mockResponse(204);
		await deletePlan(2026, 1);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans/2026/1', { method: 'DELETE' });
	});
});

// ---------------------------------------------------------------------------
// Recipe import API
// ---------------------------------------------------------------------------

describe('importFromUrl', () => {
	it('POSTs to /api/import/url with the URL in JSON body', async () => {
		const draft = { name: 'Pasta', ingredients: [], instructions: 'Boil water', imageBase64: null };
		mockResponse(200, draft);
		const result = await importFromUrl('https://example.com/recipe');
		expect(mockFetch).toHaveBeenCalledWith('/api/import/url', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ url: 'https://example.com/recipe' }),
		});
		expect(result).toEqual(draft);
	});
});

describe('importFromPaste', () => {
	it('POSTs to /api/import/paste with content in JSON body', async () => {
		const draft = { name: 'Toast', ingredients: [{ name: 'bread', quantity: null }], instructions: 'Toast it', imageBase64: null };
		mockResponse(200, draft);
		const result = await importFromPaste('<html>raw html</html>');
		expect(mockFetch).toHaveBeenCalledWith('/api/import/paste', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ content: '<html>raw html</html>' }),
		});
		expect(result).toEqual(draft);
	});
});

// ---------------------------------------------------------------------------
// Bring! shopping list API
// ---------------------------------------------------------------------------

import { sendToBring } from './api';

describe('sendToBring', () => {
	it('POSTs to /api/bring/items with name and spec', async () => {
		mockResponse(200, { sent: true });

		const result = await sendToBring('Tomatoes', '400 g');

		expect(mockFetch).toHaveBeenCalledWith('/api/bring/items', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name: 'Tomatoes', spec: '400 g' }),
		});
		expect(result).toEqual({ sent: true });
	});

	it('POSTs with spec null when no quantity', async () => {
		mockResponse(200, { sent: true });

		await sendToBring('Tomatoes', null);

		expect(mockFetch).toHaveBeenCalledWith('/api/bring/items', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name: 'Tomatoes', spec: null }),
		});
	});

	it('throws with error message from server', async () => {
		mockResponse(400, { error: 'Bring! credentials not configured: set BRING_EMAIL and BRING_PASSWORD' });

		await expect(sendToBring('Tomatoes', null)).rejects.toThrow(
			'Bring! credentials not configured'
		);
	});
});

import { checkBringStatus } from './api';

describe('checkBringStatus', () => {
	it('GETs /api/bring/status and returns the parsed body', async () => {
		mockResponse(200, { configured: true, connected: true, error: null });

		const result = await checkBringStatus();

		expect(mockFetch).toHaveBeenCalledWith('/api/bring/status', undefined);
		expect(result).toEqual({ configured: true, connected: true, error: null });
	});

	it('throws with error message on server error', async () => {
		mockResponse(500, { error: 'internal server error' });

		await expect(checkBringStatus()).rejects.toThrow('internal server error');
	});
});
