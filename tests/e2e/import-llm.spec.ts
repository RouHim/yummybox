import { test, expect, type Page } from '@playwright/test';
import { resetMeals, resetSettings, setLocale } from './_helpers';

test.describe('LLM import', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
		await resetSettings(request);
	});

	async function openLlmTab(page: Page): Promise<void> {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('button', { name: 'AI import' }).click();
	}

	test('given_llm_provider_configured_when_parse_clicked_then_form_populated_and_meal_added', async ({ page }) => {
		await page.route('**/api/llm/providers', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					providers: [
						{
							id: 'openai',
							name: 'OpenAI',
							envVar: 'OPENAI_API_KEY',
							configured: true,
							supportsCustomEndpoint: false,
						},
					],
				}),
			});
		});

		await page.route('**/api/llm/models?*', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['gpt-4o-mini'] }),
			});
		});

		await page.route('**/api/import/llm', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					name: 'AI Curry',
					ingredients: [{ name: 'chicken', quantity: '200 g' }],
					instructions: 'Cook.',
					imageBase64: null,
				}),
			});
		});

		await openLlmTab(page);

		const dialog = page.getByRole('dialog');
		const providerSelect = dialog.locator('select').first();
		await expect(providerSelect).toBeVisible();
		await providerSelect.selectOption('openai');

		const modelSelect = dialog.locator('select').nth(1);
		await expect(modelSelect).toBeVisible();
		await modelSelect.selectOption('gpt-4o-mini');

		// `openai` needs a stored key: the parse button stays disabled until the
		// stored configuration is complete. Blurring commits the key.
		const apiKey = dialog.getByLabel('API key');
		await apiKey.fill('test-key');
		await apiKey.blur();

		await dialog.locator('.llm-hint-input').fill('A spicy chicken curry');

		await page.getByRole('button', { name: 'Parse with AI' }).click();

		// After LLM import, form should be visible with populated data (auto-switched to manual tab)
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('AI Curry');

		await dialog.getByRole('button', { name: /^(Add|Hinzufügen)$/ }).click();

		await expect(dialog).not.toBeVisible();
		await expect(page.getByRole('listitem').filter({ hasText: 'AI Curry' })).toBeVisible();
	});

	test('given_no_llm_providers_when_llm_tab_opened_then_error_shown', async ({ page }) => {
		await page.route('**/api/llm/providers', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ providers: [] }),
			});
		});

		await openLlmTab(page);

		const error = page.locator('.form-error').filter({ hasText: /No LLM providers configured/ });
		await expect(error).toBeVisible();
		await expect(page.getByRole('button', { name: 'Parse with AI' })).toHaveCount(0);
	});

	test('given_failed_model_listing_when_api_key_stored_then_model_select_appears', async ({ page }) => {
		await page.route('**/api/llm/providers', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					providers: [
						{
							id: 'openai',
							name: 'OpenAI',
							envVar: 'OPENAI_API_KEY',
							configured: true,
							supportsCustomEndpoint: false,
						},
					],
				}),
			});
		});

		// The server resolves the key from the store: the listing only succeeds
		// once the key has been committed, exactly like the real backend. The
		// decision uses the key's *source*, not `set`: `set` is also true for an
		// ambient OPENAI_API_KEY in the environment, which would flip this mock
		// to 200 before the test has stored anything.
		await page.route('**/api/llm/models?*', async (route) => {
			const settings = await page.request.get('/api/settings');
			const keySource = ((await settings.json()) as { ai: { apiKey: { source: string } } }).ai.apiKey
				.source;
			if (keySource !== 'settings') {
				await route.fulfill({
					status: 400,
					contentType: 'application/json',
					body: JSON.stringify({ error: 'API key not configured', code: 'llm_not_configured' }),
				});
				return;
			}
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['gpt-4o-mini'] }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		await dialog.locator('select').first().selectOption('openai');

		// A listing without a stored key falls back to a free-text model field.
		const modelInput = dialog.getByPlaceholder('Model name (e.g. gpt-4o-mini)');
		await expect(modelInput).toBeVisible();

		// Storing the key must retry the listing and bring the select back.
		const apiKey = dialog.getByLabel('API key');
		await apiKey.fill('test-key');
		await apiKey.blur();

		const modelSelect = dialog.locator('select').nth(1);
		await expect(modelSelect).toBeVisible();
		await expect(modelSelect.locator('option[value="gpt-4o-mini"]')).toHaveCount(1);
	});

	test('given_loaded_custom_models_when_base_url_cleared_then_stale_models_are_dropped', async ({ page }) => {
		await page.route('**/api/llm/providers', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					providers: [
						{
							id: 'custom',
							name: 'Custom (OpenAI-compatible)',
							envVar: '',
							configured: true,
							supportsCustomEndpoint: true,
						},
					],
				}),
			});
		});

		await page.route('**/api/llm/models?*', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['local-model'] }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		await dialog.locator('select').first().selectOption('custom');
		await dialog.getByLabel('Base URL').fill('http://127.0.0.1:18999/v1/');
		await dialog.getByLabel('Base URL').blur();

		const modelSelect = dialog.locator('select').nth(1);
		await expect(modelSelect.locator('option[value="local-model"]')).toHaveCount(1);
		await modelSelect.selectOption('local-model');

		// Clearing the endpoint drops the models that belonged to it.
		await dialog.getByLabel('Base URL').fill('');
		await dialog.getByLabel('Base URL').blur();

		await expect(dialog.locator('select').nth(1).locator('option[value="local-model"]')).toHaveCount(0);
	});
});
