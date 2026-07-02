import { test, expect, type Page } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

test.describe('Bulk import meals', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
		await page.goto('/meals');
	});

	async function openBulkImport(page: Page) {
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('button', { name: 'Bulk URL' }).click();
	}

	test('given_bulk_import_returns_created_meals_when_imported_then_modal_closes_and_meals_appear', async ({ page }) => {
		await openBulkImport(page);

		await page.getByPlaceholder('Paste recipe URLs, one per line…').fill('https://example.com/r1\nhttps://example.com/r2');

		const fakeMeal = { id: 1, name: 'Imported Meal 1', ingredients: [{ name: 'a', quantity: null }], instructions: 'x', has_image: false };

		await page.route('**/api/import/bulk', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ created: [fakeMeal], failed: [] }),
			});
		});

		// After bulk import succeeds, the page reloads meals via GET /api/meals.
		// Stub that too so the fake meal appears in the list without a real DB write.
		await page.route('**/api/meals', async (route) => {
			if (route.request().method() === 'GET') {
				await route.fulfill({
					status: 200,
					contentType: 'application/json',
					body: JSON.stringify([fakeMeal]),
				});
			} else {
				await route.continue();
			}
		});

		await page.getByRole('button', { name: 'Import all' }).click();

		await expect(page.getByRole('dialog')).not.toBeVisible();
		await expect(page.getByRole('listitem').filter({ hasText: 'Imported Meal 1' })).toBeVisible();
	});

	test('given_bulk_import_returns_failures_when_imported_then_results_shown', async ({ page }) => {
		await openBulkImport(page);

		await page.getByPlaceholder('Paste recipe URLs, one per line…').fill('https://example.com/r1\nhttps://example.com/r2');

		await page.route('**/api/import/bulk', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					created: [],
					failed: [
						{ url: 'https://example.com/r1', reason: 'fetch failed' },
						{ url: 'https://example.com/r2', reason: 'no recipe found' },
					],
				}),
			});
		});

		await page.getByRole('button', { name: 'Import all' }).click();

		await expect(page.getByRole('dialog')).toBeVisible();
		await expect(page.locator('.bulk-results__success')).toContainText('0 meals imported');

		const failures = page.locator('.bulk-results__failures > li');
		await expect(failures).toHaveCount(2);

		const firstFailure = failures.nth(0);
		await expect(firstFailure.locator('.bulk-results__url')).toContainText('example.com/r1');
		await expect(firstFailure.locator('.bulk-results__reason')).toContainText('fetch failed');

		const secondFailure = failures.nth(1);
		await expect(secondFailure.locator('.bulk-results__url')).toContainText('example.com/r2');
		await expect(secondFailure.locator('.bulk-results__reason')).toContainText('no recipe found');

		await page.getByRole('button', { name: 'New batch' }).click();
		await expect(page.getByPlaceholder('Paste recipe URLs, one per line…')).toBeVisible();
	});
});
