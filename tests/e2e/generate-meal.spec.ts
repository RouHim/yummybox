import { test, expect } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

const TINY_PNG = Buffer.from(
	'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
	'base64',
);

async function configureMockProvider(page: import('@playwright/test').Page) {
	// Provider select is the first select in the picker.
	await page.locator('select').first().selectOption('custom');
	await page.getByLabel('Base URL').fill('http://127.0.0.1:18999/v1/');
	// genai's OpenAI adapter requires a key value even for keyless endpoints;
	// the mock ignores the Authorization header.
	await page.getByLabel('API Key (optional)').fill('mock-key');
	// Model list loads from the mock after the 500 ms debounce.
	await expect(page.locator('select').nth(1)).toBeVisible({ timeout: 10_000 });
	await page.locator('select').nth(1).selectOption('mock-model');
}

test.describe('Generate meal page', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('top bar button opens the generate page', async ({ page }) => {
		await page.goto('/');
		await page.getByRole('link', { name: 'Spontaneous cooking' }).click();
		await expect(page).toHaveURL(/\/spontaneous$/);
		await expect(page.getByRole('heading', { name: 'Spontaneous cooking' })).toBeVisible();
		// Generation must not have persisted anything.
		const res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(0);
	});

	test('generate button is disabled until model and input are provided', async ({ page }) => {
		await page.goto('/spontaneous');
		const generateBtn = page.getByRole('button', { name: /^Generate recipe$/ });
		await expect(generateBtn).toBeDisabled();
		// Ingredients alone are not enough without a model.
		await page.getByLabel(/ingredients/i).fill('flour\neggs');
		await expect(generateBtn).toBeDisabled();
		await configureMockProvider(page);
		await expect(generateBtn).toBeEnabled();
	});

	test('generates a recipe via AI and saves it as a meal', async ({ page }) => {
		await page.goto('/spontaneous');
		await configureMockProvider(page);
		await page.getByLabel(/ingredients/i).fill('flour\neggs');
		await page.getByRole('button', { name: /^Generate recipe$/ }).click();
		// Draft appears in an editable form on the same page (no persistence yet).
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Mock Pasta');
		await expect(page.getByText(/AI draft ready/)).toBeVisible();
		let res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(0);
		// Explicit save persists the meal and returns to the meals list.
		await page.getByRole('button', { name: /^(Save|Speichern)$/ }).click();
		await expect(page).toHaveURL(/\/meals/);
		await expect(page.getByRole('listitem').filter({ hasText: 'Mock Pasta' })).toBeVisible();
		res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(1);
	});

	test('generates from photos only', async ({ page }) => {
		await page.goto('/spontaneous');
		await configureMockProvider(page);
		await page.locator('input[type="file"]').setInputFiles([
			{ name: 'a.png', mimeType: 'image/png', buffer: TINY_PNG },
			{ name: 'b.png', mimeType: 'image/png', buffer: TINY_PNG },
		]);
		await page.getByRole('button', { name: /^Generate recipe$/ }).click();
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Mock Pasta');
	});

	test('rejects more than 5 photos', async ({ page }) => {
		await page.goto('/spontaneous');
		const files = Array.from({ length: 6 }, (_, i) => ({
			name: `${i}.png`,
			mimeType: 'image/png',
			buffer: TINY_PNG,
		}));
		await page.locator('input[type="file"]').setInputFiles(files);
		await expect(page.getByText(/At most 5 photos allowed/)).toBeVisible();
	});

	test('restores the provider config and collapses AI settings on revisit', async ({ page }) => {
		await page.goto('/spontaneous');
		await configureMockProvider(page);
		await page.getByLabel(/ingredients/i).fill('flour\neggs');
		await page.getByRole('button', { name: /^Generate recipe$/ }).click();
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Mock Pasta');
		await page.getByRole('button', { name: /^(Save|Speichern)$/ }).click();
		await page.getByRole('link', { name: 'Spontaneous cooking' }).click();
		// Revisit: the stored config restores and the settings block collapses,
		// leaving the ingredients input as the focus of the page.

		await expect(page).toHaveURL(/\/spontaneous$/);
		await expect(page.getByText(/Model: mock-model/)).toBeVisible();
		await expect(page.locator('select').first()).toBeHidden();
		await expect(page.getByLabel(/ingredients/i)).toBeVisible();
		// Change reveals the picker again.
		await page.getByRole('button', { name: /^Change$/ }).click();
		await expect(page.locator('select').first()).toBeVisible();
	});

	test('cooks the edited draft without persisting it', async ({ page }) => {
		await page.goto('/spontaneous');
		await configureMockProvider(page);
		await page.getByLabel(/ingredients/i).fill('flour\neggs');
		await page.getByRole('button', { name: /^Generate recipe$/ }).click();
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Mock Pasta');
		// Edits made in the form must carry over into cooking.
		await page.getByLabel('Name', { exact: true }).fill('Cooked Draft');
		await page.getByRole('button', { name: 'Cook now' }).click();
		await expect(page).toHaveURL(/\/spontaneous\/cook$/);
		await expect(page.locator('.cooking-view__name')).toHaveText('Cooked Draft');
		await expect(page.locator('.cooking-view__ingredient-list')).toContainText('flour');
		// Nothing was persisted.
		let res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(0);
		// Leaving the flow forgets the draft: the spontaneous page is fresh.
		await page.goto('/spontaneous');
		await expect(page.locator('.generate-draft')).toHaveCount(0);
		res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(0);
	});
});
