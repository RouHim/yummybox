import { test, expect, type Page } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

test.describe('Import URLs — single URL', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	async function openImportUrls(page: Page) {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('button', { name: 'Import URLs' }).click();
	}

	test('given_single_url_when_imported_then_navigates_to_detail_view', async ({ page }) => {
		await openImportUrls(page);

		const fakeMeal = { id: 42, name: 'Test Curry', ingredients: [{ name: 'chicken', quantity: '250 g' }], instructions: 'Cook.', has_image: false };

		await page.route('**/api/import/bulk', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ created: [fakeMeal], failed: [] }),
			});
		});

		// Stub the detail-page fetch so the navigation resolves
		await page.route('**/api/meals/42', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify(fakeMeal),
			});
		});

		await page.getByPlaceholder('Paste recipe URLs, one per line…').fill('https://example.com/curry');
		await page.getByRole('button', { name: 'Import all' }).click();

		// Should navigate to detail page
		await expect(page).toHaveURL(/\/meals\/42/);
		await expect(page.getByRole('dialog')).not.toBeVisible();
	});

	test('given_network_error_when_imported_then_error_shown_and_modal_stays', async ({ page }) => {
		await openImportUrls(page);

		await page.route('**/api/import/bulk', async (route) => {
			await route.fulfill({
				status: 500,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'Internal server error' }),
			});
		});

		await page.getByPlaceholder('Paste recipe URLs, one per line…').fill('https://example.com/broken');
		await page.getByRole('button', { name: 'Import all' }).click();

		await expect(page.getByRole('dialog')).toBeVisible();
		await expect(page.locator('.form-error')).toBeVisible();
	});
});
