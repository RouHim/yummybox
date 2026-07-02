import { test, expect, type Page } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

test.describe('LLM import', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
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

		await dialog.locator('.llm-hint-input').fill('A spicy chicken curry');

		await page.getByRole('button', { name: 'Parse with AI' }).click();

		await expect(page.getByRole('button', { name: /^Import another$/ })).toBeVisible();
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
});
