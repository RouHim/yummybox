import { test, expect } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

test.describe('Import from paste', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('given_raw_html_when_imported_then_form_populated_and_meal_added', async ({ page }) => {
		await page.route('**/api/import/paste', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					name: 'Pasted Recipe',
					ingredients: [{ name: 'sugar', quantity: '100 g' }],
					instructions: 'Mix well.',
					imageBase64: null,
				}),
			});
		});

		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();

		// Non-URL input (no http:// prefix) dispatches to /api/import/paste
		await page.getByRole('textbox', { name: 'Recipe URL or raw HTML/JSON-LD' }).fill(
			'<html><script type="application/ld+json">{"@type":"Recipe","name":"Pasted Recipe"}</script></html>',
		);
		await page.getByRole('button', { name: 'Fetch & parse' }).click();

		await expect(page.getByRole('button', { name: /^Import another$/ })).toBeVisible();
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Pasted Recipe');

		await page.getByRole('dialog').getByRole('button', { name: /^(Add|Hinzufügen)$/ }).click();

		await expect(page.getByRole('dialog')).not.toBeVisible();
		await expect(page.getByRole('listitem').filter({ hasText: 'Pasted Recipe' })).toBeVisible();
	});
});
