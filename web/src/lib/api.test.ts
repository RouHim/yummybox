import { describe, it, expect, vi, beforeEach } from 'vitest';
import { listMeals, getMeal, createMeal, updateMeal, deleteMeal, mealImageUrl, listPlansForYear, getPlan, createPlan, updatePlan, deletePlan, importFromUrl, importFromPaste, importFromLlm, generateMeal, importBulk, listLlmProviders, listLlmModels, polishInstructions, getVersion, ApiError } from './api';
import type { Meal, MealPayload, NewIngredientLine, Plan, NewPlanRequest, PlanPatch } from './types';

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
		expect(mockFetch).toHaveBeenCalledWith('/api/meals', expect.objectContaining({ signal: expect.any(AbortSignal) }));
	});

	it('calls /api/meals?search=...', async () => {
		mockResponse(200, []);
		await listMeals('pizza');
		expect(mockFetch).toHaveBeenCalledWith('/api/meals?search=pizza', expect.objectContaining({ signal: expect.any(AbortSignal) }));
	});
});

describe('getMeal', () => {
	it('calls /api/meals/:id', async () => {
		const meal: Meal = { id: 5, name: 'Pasta', ingredients: [], last_planned_at: null, created_at: '', updated_at: '', has_image: false, instructions: '', portions: null };
		mockResponse(200, meal);
		const result = await getMeal(5);
		expect(mockFetch).toHaveBeenCalledWith('/api/meals/5', expect.objectContaining({ signal: expect.any(AbortSignal) }));
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
		const mealResponse: Meal = { id: 1, name: 'Test', ingredients: [{ name: 'stuff', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: false, instructions: '', portions: null };
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
		const mealResponse: Meal = { id: 2, name: 'Pizza', ingredients: [{ name: 'cheese', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: true, instructions: '', portions: null };
		mockResponse(201, mealResponse);
		const file = new File([new Uint8Array([1, 2, 3])], 'photo.png', { type: 'image/png' });
		await createMeal(payload, file);
		const fd = mockFetch.mock.calls[0][1].body as FormData;
		expect(fd.get('image')).toBeInstanceOf(File);
		expect((fd.get('image') as File).name).toBe('photo.png');
	});

	it('handles null image gracefully', async () => {
		const payload: MealPayload = { name: 'X', ingredients: [{ name: 'y', quantity: null }] , instructions: '' };
		const mealResponse: Meal = { id: 3, name: 'X', ingredients: [{ name: 'y', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: false, instructions: '', portions: null };
		mockResponse(201, mealResponse);
		await createMeal(payload, null);
		const fd = mockFetch.mock.calls[0][1].body as FormData;
		expect(fd.get('image')).toBeNull();
	});
});

describe('updateMeal', () => {
	it('sends multipart form with name and ingredients', async () => {
		const payload: MealPayload = { name: 'Updated', ingredients: [{ name: 'new', quantity: null }] , instructions: '' };
		const mealResponse: Meal = { id: 3, name: 'Updated', ingredients: [{ name: 'new', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: false, instructions: '', portions: null };
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
		const mealResponse: Meal = { id: 4, name: 'X', ingredients: [{ name: 'y', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: false, instructions: '', portions: null };
		mockResponse(200, mealResponse);
		await updateMeal(4, payload, { removeImage: true });
		const fd = mockFetch.mock.calls[0][1].body as FormData;
		expect(fd.get('image_action')).toBe('remove');
	});

	it('sends image file when replacing', async () => {
		const payload: MealPayload = { name: 'X', ingredients: [{ name: 'y', quantity: null }] , instructions: '' };
		const mealResponse: Meal = { id: 5, name: 'X', ingredients: [{ name: 'y', quantity: null }], last_planned_at: null, created_at: '', updated_at: '', has_image: true, instructions: '', portions: null };
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
		expect(mockFetch).toHaveBeenCalledWith('/api/meals/7', expect.objectContaining({ method: 'DELETE', signal: expect.any(AbortSignal) }));
	});
});

describe('mealImageUrl', () => {
	it('returns the correct image endpoint URL', () => {
		expect(mealImageUrl(42)).toBe('/api/meals/42/image');
	});
});

describe('error handling', () => {
    it('extracts server error message from JSON body', async () => {
        mockResponse(400, { error: 'name must not be empty', code: null });
        await expect(listMeals()).rejects.toThrow('name must not be empty');
    });

    it('throws ApiError with code when code is present', async () => {
        mockResponse(500, { error: 'timed out', code: 'llm_timeout' });
        try {
            await listMeals();
            expect.fail('should have thrown');
        } catch (err) {
            expect(err).toBeInstanceOf(ApiError);
            expect((err as ApiError).message).toBe('timed out');
            expect((err as ApiError).code).toBe('llm_timeout');
        }
    });

    it('throws ApiError with null code when no code in body', async () => {
        mockResponse(400, { error: 'bad request' });
        try {
            await listMeals();
            expect.fail('should have thrown');
        } catch (err) {
            expect(err).toBeInstanceOf(ApiError);
            expect((err as ApiError).code).toBeNull();
        }
    });

    it('throws ApiError with status 409 for duplicate name', async () => {
        mockResponse(409, { error: 'a meal with this name already exists', code: null });
        try {
            await listMeals();
            expect.fail('should have thrown');
        } catch (err) {
            expect(err).toBeInstanceOf(ApiError);
            expect((err as ApiError).status).toBe(409);
            expect((err as ApiError).message).toBe('a meal with this name already exists');
        }
    });
});

describe('network failures and retries', () => {
    it('retries GET failures and rejects with REQUEST_FAILED after exhausting retries', async () => {
        mockFetch.mockRejectedValue(new TypeError('Failed to fetch'));
        try {
            await listMeals();
            expect.fail('should have thrown');
        } catch (err) {
            expect(err).toBeInstanceOf(ApiError);
            expect((err as ApiError).code).toBe('REQUEST_FAILED');
            expect((err as ApiError).message).toContain('connection');
        }
        expect(mockFetch).toHaveBeenCalledTimes(3);
    });

    it('recovers from transient GET failures and resolves', async () => {
        mockFetch.mockRejectedValueOnce(new TypeError('Failed to fetch'));
        mockFetch.mockRejectedValueOnce(new TypeError('Failed to fetch'));
        mockResponse(200, []);
        const result = await listMeals();
        expect(result).toEqual([]);
        expect(mockFetch).toHaveBeenCalledTimes(3);
    });

    it('never retries HTTP error responses', async () => {
        mockResponse(500, { error: 'boom' });
        try {
            await getMeal(5);
            expect.fail('should have thrown');
        } catch (err) {
            expect(err).toBeInstanceOf(ApiError);
            expect((err as ApiError).status).toBe(500);
            expect((err as ApiError).message).toBe('boom');
        }
        expect(mockFetch).toHaveBeenCalledTimes(1);
    });

    it('never retries POST requests', async () => {
        mockFetch.mockRejectedValueOnce(new TypeError('Failed to fetch'));
        try {
            await createMeal({ name: 'X', ingredients: [], instructions: '' });
            expect.fail('should have thrown');
        } catch (err) {
            expect(err).toBeInstanceOf(ApiError);
            expect((err as ApiError).code).toBe('REQUEST_FAILED');
        }
        expect(mockFetch).toHaveBeenCalledTimes(1);
    });

    it('times out after 30s and rejects with REQUEST_FAILED', async () => {
        vi.useFakeTimers();
        try {
            mockFetch.mockImplementation(
                (_url: unknown, init?: RequestInit) =>
                    new Promise((_, reject) => {
                        init?.signal?.addEventListener('abort', () =>
                            reject(new DOMException('The operation was aborted.', 'AbortError'))
                        );
                    })
            );
            const promise = createMeal({ name: 'X', ingredients: [], instructions: '' });
            const rejection = expect(promise).rejects.toMatchObject({ code: 'REQUEST_FAILED' });
            await vi.advanceTimersByTimeAsync(30_000);
            await rejection;
            expect(mockFetch).toHaveBeenCalledTimes(1);
        } finally {
            vi.useRealTimers();
        }
    });
});

// ---------------------------------------------------------------------------
// Plan API
// ---------------------------------------------------------------------------

describe('listPlansForYear', () => {
	it('calls /api/plans?year=...', async () => {
		mockResponse(200, []);
		await listPlansForYear(2026);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans?year=2026', expect.objectContaining({ signal: expect.any(AbortSignal) }));
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
		expect(mockFetch).toHaveBeenCalledWith('/api/plans?year=2026&week=1', expect.objectContaining({ signal: expect.any(AbortSignal) }));
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
		expect(mockFetch).toHaveBeenCalledWith('/api/plans', expect.objectContaining({
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(payload),
			signal: expect.any(AbortSignal)
		}));
	});
});

describe('updatePlan', () => {
	it('puts JSON body', async () => {
		const payload: PlanPatch = { meal_ids: [1, 2] };
		const plan: Plan = { id: 1, year: 2026, week_number: 1, created_at: '', meals: [], ingredient_summary: [] };
		mockResponse(200, plan);
		await updatePlan(2026, 1, payload);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans/2026/1', expect.objectContaining({
			method: 'PUT',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify(payload),
			signal: expect.any(AbortSignal)
		}));
	});
});

describe('deletePlan', () => {
	it('deletes /api/plans/:year/:week', async () => {
		mockResponse(204);
		await deletePlan(2026, 1);
		expect(mockFetch).toHaveBeenCalledWith('/api/plans/2026/1', expect.objectContaining({ method: 'DELETE', signal: expect.any(AbortSignal) }));
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
		expect(mockFetch).toHaveBeenCalledWith('/api/import/url', expect.objectContaining({
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ url: 'https://example.com/recipe' }),
			signal: expect.any(AbortSignal)
		}));
		expect(result).toEqual(draft);
	});
});

describe('importFromPaste', () => {
	it('POSTs to /api/import/paste with content in JSON body', async () => {
		const draft = { name: 'Toast', ingredients: [{ name: 'bread', quantity: null }], instructions: 'Toast it', imageBase64: null };
		mockResponse(200, draft);
		const result = await importFromPaste('<html>raw html</html>');
		expect(mockFetch).toHaveBeenCalledWith('/api/import/paste', expect.objectContaining({
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ content: '<html>raw html</html>' }),
			signal: expect.any(AbortSignal)
		}));
		expect(result).toEqual(draft);
	});
});


// ---------------------------------------------------------------------------
// Bulk import API
// ---------------------------------------------------------------------------

describe('importBulk', () => {
	it('POSTs to /api/import/bulk with urls array', async () => {
		const result = { created: [], failed: [] };
		mockResponse(200, result);

		const response = await importBulk({ urls: ['https://example.com/a', 'https://example.com/b'] });
		expect(mockFetch).toHaveBeenCalledWith('/api/import/bulk', expect.objectContaining({
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ urls: ['https://example.com/a', 'https://example.com/b'] }),
			signal: expect.any(AbortSignal)
		}));
		expect(response).toEqual(result);
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

		expect(mockFetch).toHaveBeenCalledWith('/api/bring/items', expect.objectContaining({
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name: 'Tomatoes', spec: '400 g' }),
			signal: expect.any(AbortSignal)
		}));
		expect(result).toEqual({ sent: true });
	});

	it('POSTs with spec null when no quantity', async () => {
		mockResponse(200, { sent: true });

		await sendToBring('Tomatoes', null);

		expect(mockFetch).toHaveBeenCalledWith('/api/bring/items', expect.objectContaining({
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name: 'Tomatoes', spec: null }),
			signal: expect.any(AbortSignal)
		}));
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

		expect(mockFetch).toHaveBeenCalledWith('/api/bring/status', expect.objectContaining({ signal: expect.any(AbortSignal) }));
		expect(result).toEqual({ configured: true, connected: true, error: null });
	});

	it('throws with error message on server error', async () => {
		mockResponse(500, { error: 'internal server error' });

		await expect(checkBringStatus()).rejects.toThrow('internal server error');
	});
});


// ---------------------------------------------------------------------------
// LLM provider & model listing API
// ---------------------------------------------------------------------------

describe('listLlmProviders', () => {
    it('calls GET /api/llm/providers', async () => {
        const providers = [{ id: 'openai', name: 'OpenAI', envVar: 'OPENAI_API_KEY', configured: false, supportsCustomEndpoint: false }];
        mockResponse(200, { providers });
        const result = await listLlmProviders();
        expect(mockFetch).toHaveBeenCalledWith('/api/llm/providers', expect.objectContaining({ signal: expect.any(AbortSignal) }));
        expect(result).toEqual(providers);
    });
});

describe('listLlmModels', () => {
    it('calls GET /api/llm/models?provider=openai', async () => {
        const models = { models: ['gpt-4o-mini', 'gpt-4o'] };
        mockResponse(200, models);
        const result = await listLlmModels('openai');
        expect(mockFetch).toHaveBeenCalledWith('/api/llm/models?provider=openai', expect.objectContaining({ signal: expect.any(AbortSignal) }));
        expect(result).toEqual(models);
    });

    it('includes base_url and api_key for custom providers', async () => {
        const models = { models: ['local-model'] };
        mockResponse(200, models);
        await listLlmModels('custom', 'http://localhost:8080/v1/', 'sk-key');
        const url = mockFetch.mock.calls[0][0] as string;
        expect(url).toContain('provider=custom');
        expect(url).toContain('base_url=http%3A%2F%2Flocalhost%3A8080%2Fv1%2F');
        expect(url).toContain('api_key=sk-key');
    });
});

describe('importFromLlm', () => {
    it('sends model, hint, image in multipart form', async () => {
        const draft = { name: 'Pasta', ingredients: [], instructions: '', imageBase64: null };
        mockResponse(200, draft);
        const file = new File([new Uint8Array([1])], 'photo.jpg', { type: 'image/jpeg' });
        await importFromLlm('gpt-4o-mini', 'pasta dish', [file]);
        expect(mockFetch).toHaveBeenCalledTimes(1);
        const [url, opts] = mockFetch.mock.calls[0];
        expect(url).toBe('/api/import/llm');
        expect(opts.method).toBe('POST');
        const fd = opts.body as FormData;
        expect(fd.get('model')).toBe('gpt-4o-mini');
        expect(fd.get('hint')).toBe('pasta dish');
        expect(fd.getAll('image')).toHaveLength(1);
        expect(fd.getAll('image')[0]).toBeInstanceOf(File);
    });

    it('sends base_url and api_key for custom endpoints', async () => {
        const draft = { name: 'Pasta', ingredients: [], instructions: '', imageBase64: null };
        mockResponse(200, draft);
        await importFromLlm('local-model', null, [], 'http://localhost:8080/v1/', 'sk-123');
        const fd = mockFetch.mock.calls[0][1].body as FormData;
        expect(fd.get('base_url')).toBe('http://localhost:8080/v1/');
        expect(fd.get('api_key')).toBe('sk-123');
    });

    it('omits base_url and api_key when not provided', async () => {
        const draft = { name: 'Pasta', ingredients: [], instructions: '', imageBase64: null };
        mockResponse(200, draft);
        await importFromLlm('gpt-4o-mini', 'pasta', []);
        const fd = mockFetch.mock.calls[0][1].body as FormData;
        expect(fd.get('base_url')).toBeNull();
        expect(fd.get('api_key')).toBeNull();
    });

    it('appends multiple images in order', async () => {
        const draft = { name: 'Pasta', ingredients: [], instructions: '', imageBase64: null };
        mockResponse(200, draft);
        const front = new File([new Uint8Array([1])], 'front.jpg', { type: 'image/jpeg' });
        const back = new File([new Uint8Array([2])], 'back.jpg', { type: 'image/jpeg' });
        const extra = new File([new Uint8Array([3])], 'extra.jpg', { type: 'image/jpeg' });
        await importFromLlm('gpt-4o-mini', 'front and back', [front, back, extra]);
        const fd = mockFetch.mock.calls[0][1].body as FormData;
        const parts = fd.getAll('image');
        expect(parts).toHaveLength(3);
        expect(parts[0]).toBe(front);
        expect(parts[1]).toBe(back);
        expect(parts[2]).toBe(extra);
    });
});

// ---------------------------------------------------------------------------
// Polish instructions API
// ---------------------------------------------------------------------------

describe('polishInstructions', () => {
    it('sends model, name, ingredients, and instructions as multipart', async () => {
        mockResponse(200, { instructions: '<p>Step 1</p>' });
        const ings: NewIngredientLine[] = [{ name: 'flour', quantity: '200g' }];
        await polishInstructions('gpt-4o-mini', 'Cake', ings, 'Mix everything');
        expect(mockFetch).toHaveBeenCalledTimes(1);
        const [url, opts] = mockFetch.mock.calls[0];
        expect(url).toBe('/api/llm/polish');
        expect(opts.method).toBe('POST');
        const fd = opts.body as FormData;
        expect(fd.get('model')).toBe('gpt-4o-mini');
        expect(fd.get('name')).toBe('Cake');
        expect(fd.get('ingredients')).toBe(JSON.stringify(ings));
        expect(fd.get('instructions')).toBe('Mix everything');
    });

    it('returns the polished instructions from response', async () => {
        mockResponse(200, { instructions: '<p>Step 1. Mix.</p>' });
        const ings: NewIngredientLine[] = [{ name: 'salt', quantity: null }];
        const result = await polishInstructions('gpt-4o', 'Soup', ings, 'Add salt');
        expect(result).toBe('<p>Step 1. Mix.</p>');
    });

    it('sends base_url and api_key when provided', async () => {
        mockResponse(200, { instructions: '<p>ok</p>' });
        const ings: NewIngredientLine[] = [];
        await polishInstructions('local-model', 'Test', ings, 'do it', 'http://localhost:8080/v1/', 'sk-123');
        const fd = mockFetch.mock.calls[0][1].body as FormData;
        expect(fd.get('base_url')).toBe('http://localhost:8080/v1/');
        expect(fd.get('api_key')).toBe('sk-123');
    });

    it('omits base_url and api_key when not provided', async () => {
        mockResponse(200, { instructions: '<p>ok</p>' });
        const ings: NewIngredientLine[] = [{ name: 'x', quantity: null }];
        await polishInstructions('gpt-4o', 'Test', ings, 'do it');
        const fd = mockFetch.mock.calls[0][1].body as FormData;
        expect(fd.get('base_url')).toBeNull();
        expect(fd.get('api_key')).toBeNull();
    });
});

// ---------------------------------------------------------------------------
// Version API
// ---------------------------------------------------------------------------

describe('getVersion', () => {
	it('calls /api/version and returns version', async () => {
		mockResponse(200, { version: '0.1.0' });
		const result = await getVersion();
		expect(mockFetch).toHaveBeenCalledWith('/api/version', expect.objectContaining({ signal: expect.any(AbortSignal) }));
		expect(result.version).toBe('0.1.0');
	});
});

describe('generateMeal', () => {
	it('sends model, ingredients and multiple images in multipart form', async () => {
		const draft = { name: 'Pasta', ingredients: [], instructions: '', imageBase64: null, portions: null };
		mockResponse(200, draft);
		const img1 = new File([new Uint8Array([1])], 'a.jpg', { type: 'image/jpeg' });
		const img2 = new File([new Uint8Array([2])], 'b.png', { type: 'image/png' });
		await generateMeal('mock-model', 'flour\neggs', [img1, img2]);
		expect(mockFetch).toHaveBeenCalledTimes(1);
		const [url, opts] = mockFetch.mock.calls[0];
		expect(url).toBe('/api/import/generate');
		expect(opts.method).toBe('POST');
		const fd = opts.body as FormData;
		expect(fd.get('model')).toBe('mock-model');
		expect(fd.get('ingredients')).toBe('flour\neggs');
		const images = fd.getAll('image');
		expect(images).toHaveLength(2);
		expect(images[0]).toBe(img1);
		expect(images[1]).toBe(img2);
	});

	it('omits empty ingredients and includes custom endpoint fields', async () => {
		mockResponse(200, { name: '', ingredients: [], instructions: '', imageBase64: null, portions: null });
		await generateMeal('m', '   ', [], 'http://localhost:8080/v1/', 'sk-123');
		const fd = mockFetch.mock.calls[0][1].body as FormData;
		expect(fd.get('ingredients')).toBeNull();
		expect(fd.get('base_url')).toBe('http://localhost:8080/v1/');
		expect(fd.get('api_key')).toBe('sk-123');
	});
});
