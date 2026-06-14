import type { Meal, MealPayload } from './types';

async function request<T>(url: string, options?: RequestInit): Promise<T> {
	const response = await fetch(url, options);
	if (!response.ok) {
		let message = '__REQUEST_FAILED__';
		try {
			const body = await response.json();
			if (body && typeof body.error === 'string') {
				message = body.error;
			}
		} catch {
			// Response was not JSON; fall back to status text
		}
		throw new Error(message);
	}
	if (response.status === 204) {
		return undefined as T;
	}
	return response.json() as Promise<T>;
}

export async function listMeals(search?: string): Promise<Meal[]> {
	const url = search ? `/api/meals?search=${encodeURIComponent(search)}` : '/api/meals';
	return request<Meal[]>(url);
}

export async function createMeal(payload: MealPayload): Promise<Meal> {
	return request<Meal>('/api/meals', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(payload)
	});
}

export async function updateMeal(id: number, payload: MealPayload): Promise<Meal> {
	return request<Meal>(`/api/meals/${id}`, {
		method: 'PUT',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(payload)
	});
}

export async function deleteMeal(id: number): Promise<void> {
	return request<void>(`/api/meals/${id}`, {
		method: 'DELETE'
	});
}
