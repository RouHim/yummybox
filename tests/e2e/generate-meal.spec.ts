import { test, expect, type Page } from '@playwright/test';
import { resetMeals, resetSettings, setLocale } from './_helpers';

const TINY_PNG = Buffer.from(
	'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
	'base64',
);

async function configureMockProvider(page: Page) {
	// Provider select is the first select in the picker.
	await page.locator('select').first().selectOption('custom');
	await page.getByLabel('Base URL').fill('http://127.0.0.1:18999/v1/');
	// genai's OpenAI adapter requires a key value even for keyless endpoints;
	// the mock ignores the Authorization header.
	await page.getByLabel('API Key (optional)').fill('mock-key');
	// Blurring commits the base URL and the key, which is what triggers the
	// model list request against the now stored configuration.
	await page.getByLabel('API Key (optional)').blur();
	await expect(page.locator('select').nth(1)).toBeVisible({ timeout: 10_000 });
	await page.locator('select').nth(1).selectOption('mock-model');
}

test.describe('Generate meal page', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
		await resetSettings(request);
		// The app bar probes the Bring! status on every mount, and the server
		// resolves the stored-then-environment credentials for it: on a shell
		// that exports BRING_EMAIL/BRING_PASSWORD that probe would log in to the
		// real Bring! API. Answer it locally so the suite stays hermetic.
		await page.route('**/api/bring/status', (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ configured: false, connected: false, error: null }),
			})
		);
	});

	test('top bar button opens the generate page', async ({ page }) => {
		await page.goto('/');
		await page.getByRole('link', { name: 'Spontaneous cooking' }).click();
		await expect(page).toHaveURL(/\/spontaneous$/);
		await expect(page.getByRole('heading', { name: 'Spontaneous cooking' })).toBeVisible();
		// Generation must not have persisted anything: the meals list is still empty.
		await page.goto('/meals');
		await expect(page.getByText('No meals yet. Add your first one.')).toBeVisible();
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
		// The draft lives only in the form: no meal with its name is persisted yet.
		const res = await page.request.get('/api/meals?search=Mock Pasta');
		const meals = (await res.json()) as Array<{ name: string }>;
		expect(meals.some((m) => m.name === 'Mock Pasta')).toBe(false);
		// Explicit save persists the meal and returns to the meals list.
		await page.getByRole('button', { name: /^(Save|Speichern)$/ }).click();
		await expect(page).toHaveURL(/\/meals/);
		const saved = page.getByRole('listitem').filter({ hasText: 'Mock Pasta' });
		await expect(saved).toBeVisible();
		// The saved meal shows its ingredient preview in the list.
		await expect(saved).toContainText('flour');
	});

	test('waits for the configuration commit its own click issued before generating', async ({ page }) => {
		// Choosing the model is part of the same click that completes the
		// configuration, and that click also sends the generate request. The
		// request must wait for the model's commit to be answered: one that
		// overtakes it is refused with `llm_not_configured`, so the user who just
		// chose a model reads "AI is not configured" and has to click twice.
		await page.goto('/spontaneous');
		// Configure the provider but leave the model unset, so the selection
		// below is the one that completes the configuration.
		await page.locator('select').first().selectOption('custom');
		await page.getByLabel('Base URL').fill('http://127.0.0.1:18999/v1/');
		await page.getByLabel('API Key (optional)').fill('mock-key');
		await page.getByLabel('API Key (optional)').blur();
		const modelSelect = page.locator('#llm-model');
		await expect(modelSelect).toBeVisible({ timeout: 10_000 });
		await page.getByLabel(/ingredients/i).fill('flour\neggs');

		// Hold the model's commit, and count the generate requests: without the
		// wait the request leaves while that commit is still unanswered.
		const storedModel = Promise.withResolvers<void>();
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"model":"mock-model"')) {
				await route.continue();
				return;
			}
			await storedModel.promise;
			await route.continue();
		});
		let generates = 0;
		await page.route('**/api/import/generate', async (route) => {
			generates++;
			await route.continue();
		});

		await modelSelect.selectOption('mock-model');
		const generateBtn = page.getByRole('button', { name: /^Generate recipe$/ });
		await expect(generateBtn).toBeEnabled();
		await generateBtn.click();

		await page.waitForTimeout(300);
		expect(generates).toBe(0);

		storedModel.resolve();
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Mock Pasta');
		expect(generates).toBe(1);
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
		await expect(page).toHaveURL(/\/meals/);
		await expect(page.getByRole('listitem').filter({ hasText: 'Mock Pasta' })).toBeVisible();
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
		// Nothing was persisted: the edited draft name never reaches the meals list.
		let res = await page.request.get('/api/meals?search=Cooked Draft');
		let meals = (await res.json()) as Array<{ name: string }>;
		expect(meals.some((m) => m.name === 'Cooked Draft')).toBe(false);
		// Leaving the flow forgets the draft: the spontaneous page is fresh.
		await page.goto('/spontaneous');
		await expect(page.locator('.generate-draft')).toHaveCount(0);
		// Still nothing persisted after leaving the flow.
		res = await page.request.get('/api/meals?search=Cooked Draft');
		meals = (await res.json()) as Array<{ name: string }>;
		expect(meals.some((m) => m.name === 'Cooked Draft')).toBe(false);
	});
});
