import type { Meal, MealPayload, NewIngredientLine, ImportDraft, Plan, PlanSummaryItem, NewPlanRequest, PlanPatch, LlmProviderInfo, LlmModelsResponse, BulkImportRequest, BulkImportResult } from './types';


export class ApiError extends Error {
    constructor(message: string, public code: string | null) {
        super(message);
        this.name = 'ApiError';
    }
}
async function request<T>(url: string, options?: RequestInit): Promise<T> {
    const response = await fetch(url, options);
    if (!response.ok) {
        let message = '__REQUEST_FAILED__';
        let code: string | null = null;
        try {
            const body = await response.json();
            if (body && typeof body.error === 'string') {
                message = body.error;
            }
            if (body && typeof body.code === 'string') {
                code = body.code;
            }
        } catch {
            // Response was not JSON; fall back to status text
        }
        throw new ApiError(message, code);
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

export async function getMeal(id: number): Promise<Meal> {
	return request<Meal>(`/api/meals/${id}`);
}

export async function createMeal(
	payload: MealPayload,
	image?: File | null,
): Promise<Meal> {
	const form = new FormData();
	form.set('name', payload.name);
	form.set('ingredients', JSON.stringify(payload.ingredients));
	form.set('instructions', payload.instructions);
	if (image) form.set('image', image);
	return request<Meal>('/api/meals', { method: 'POST', body: form });
}

export async function updateMeal(
	id: number,
	payload: MealPayload,
	opts?: { image?: File | null; removeImage?: boolean },
): Promise<Meal> {
	const form = new FormData();
	form.set('name', payload.name);
	form.set('ingredients', JSON.stringify(payload.ingredients));
	form.set('instructions', payload.instructions);
	if (opts?.image) form.set('image', opts.image);
	if (opts?.removeImage) form.set('image_action', 'remove');
	return request<Meal>(`/api/meals/${id}`, { method: 'PUT', body: form });
}

export async function deleteMeal(id: number): Promise<void> {
	return request<void>(`/api/meals/${id}`, {
		method: 'DELETE'
	});
}

export function mealImageUrl(id: number): string {
	return `/api/meals/${id}/image`;
}

export async function loadImageFromUrl(url: string): Promise<{ imageBase64: string }> {
	return request<{ imageBase64: string }>('/api/import/image-url', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ url }),
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

// Recipe import API

export async function importFromUrl(url: string, imageUrl?: string): Promise<ImportDraft> {
	return request<ImportDraft>('/api/import/url', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ url, ...(imageUrl ? { imageUrl } : {}) }),
	});
}

export async function importFromPaste(content: string, imageUrl?: string): Promise<ImportDraft> {
	return request<ImportDraft>('/api/import/paste', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ content, ...(imageUrl ? { imageUrl } : {}) }),
	});
}
export async function importFromLlm(
    model: string,
    hint: string | null,
    image: File | null,
    baseUrl?: string,
    apiKey?: string,
): Promise<ImportDraft> {
    const form = new FormData();
    form.set('model', model);
    if (hint && hint.trim()) form.set('hint', hint.trim());
    if (image) form.set('image', image);
    if (baseUrl) form.set('base_url', baseUrl);
    if (apiKey) form.set('api_key', apiKey);
    return request<ImportDraft>('/api/import/llm', { method: 'POST', body: form });
}

export async function listLlmProviders(): Promise<LlmProviderInfo[]> {
    const data = await request<{ providers: LlmProviderInfo[] }>('/api/llm/providers');
    return data.providers;
}

export async function listLlmModels(provider: string, baseUrl?: string, apiKey?: string): Promise<LlmModelsResponse> {
    const params = new URLSearchParams({ provider });
    if (baseUrl) params.set('base_url', baseUrl);
    if (apiKey) params.set('api_key', apiKey);
    return request<LlmModelsResponse>(`/api/llm/models?${params}`);
}

export async function polishInstructions(
    model: string,
    name: string,
    ingredients: NewIngredientLine[],
    instructions: string,
    baseUrl?: string,
    apiKey?: string,
): Promise<string> {
    const form = new FormData();
    form.set('model', model);
    form.set('name', name);
    form.set('ingredients', JSON.stringify(ingredients));
    form.set('instructions', instructions);
    if (baseUrl) form.set('base_url', baseUrl);
    if (apiKey) form.set('api_key', apiKey);
    const data = await request<{ instructions: string }>('/api/llm/polish', { method: 'POST', body: form });
    return data.instructions;
}

// Bring! shopping list API

export interface BringItemResponse {
	sent: boolean;
}

export async function sendToBring(name: string, spec: string | null): Promise<BringItemResponse> {
	return request<BringItemResponse>('/api/bring/items', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ name, spec }),
	});
}

export interface BringStatusResponse {
	configured: boolean;
	connected: boolean;
	error: string | null;
}

export async function checkBringStatus(): Promise<BringStatusResponse> {
	return request<BringStatusResponse>('/api/bring/status');
}

export async function importBulk(payload: BulkImportRequest): Promise<BulkImportResult> {
	return request<BulkImportResult>('/api/import/bulk', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(payload),
	});
}
