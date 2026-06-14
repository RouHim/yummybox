import { describe, it, expect, vi, beforeEach } from 'vitest';
import { listMeals, createMeal, updateMeal, deleteMeal } from './api';
import type { Meal, MealPayload } from './types';

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
	it('POSTs payload to /api/meals', async () => {
		const payload: MealPayload = { name: 'Test', ingredients: 'stuff' };
		mockResponse(201, { id: 1, ...payload, created_at: '', updated_at: '' } as Meal);
		await createMeal(payload);
		expect(mockFetch).toHaveBeenCalledWith('/api/meals', {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(payload)
		});
	});
});

describe('updateMeal', () => {
	it('PUTs payload to /api/meals/:id', async () => {
		const payload: MealPayload = { name: 'Updated', ingredients: 'new' };
		mockResponse(200, { id: 3, ...payload, created_at: '', updated_at: '' } as Meal);
		await updateMeal(3, payload);
		expect(mockFetch).toHaveBeenCalledWith('/api/meals/3', {
			method: 'PUT',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(payload)
		});
	});
});

describe('deleteMeal', () => {
	it('DELETEs /api/meals/:id', async () => {
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
		await expect(createMeal({ name: '', ingredients: 'x' })).rejects.toThrow('name must not be empty');
	});
});
