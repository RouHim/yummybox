import type { Meal, MealPayload, Plan, PlanSummaryItem, NewPlanRequest, PlanPatch } from './types';

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

// Plan API

export async function listPlansForYear(year: number): Promise<PlanSummaryItem[]> {
	const raw = await request<unknown>(`/api/plans?year=${year}`);
	if (!Array.isArray(raw)) throw new Error('expected array');
	return raw as PlanSummaryItem[];
}

export async function getPlan(year: number, week: number): Promise<Plan | null> {
	const response = await fetch(`/api/plans?year=${year}&week=${week}`);
	if (response.status === 404) return null;
	if (!response.ok) {
		let message = '__REQUEST_FAILED__';
		try {
			const body = await response.json();
			if (body && typeof body.error === 'string') message = body.error;
		} catch {
			// response was not JSON
		}
		throw new Error(message);
	}
	if (response.status === 204) return null;
	const raw = (await response.json()) as unknown;
	if (Array.isArray(raw)) throw new Error('expected plan, got array');
	return raw as Plan;
}

export async function createPlan(payload: NewPlanRequest): Promise<Plan> {
	return request<Plan>('/api/plans', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(payload)
	});
}

export async function updatePlan(year: number, week: number, payload: PlanPatch): Promise<Plan> {
	return request<Plan>(`/api/plans/${year}/${week}`, {
		method: 'PUT',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify(payload)
	});
}

export async function deletePlan(year: number, week: number): Promise<void> {
	return request<void>(`/api/plans/${year}/${week}`, {
		method: 'DELETE'
	});
}
