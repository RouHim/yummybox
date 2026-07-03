import { test, expect, type Page } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

test.describe('Import URLs — bulk', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
		await page.goto('/meals');
	});

	async function openImportUrls(page: Page) {
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('button', { name: 'Import URLs' }).click();
	}

	test('given_multiple_urls_when_imported_then_modal_closes_and_meals_appear', async ({ page }) => {
		await openImportUrls(page);

		await page.getByPlaceholder('Paste recipe URLs, one per line…').fill('https://example.com/r1\nhttps://example.com/r2');

		const fakeMeal1 = { id: 1, name: 'Imported Meal 1', ingredients: [{ name: 'a', quantity: null }], instructions: 'x', has_image: false };
		const fakeMeal2 = { id: 2, name: 'Imported Meal 2', ingredients: [{ name: 'b', quantity: null }], instructions: 'y', has_image: false };

		await page.route('**/api/import/bulk', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ created: [fakeMeal1, fakeMeal2], failed: [] }),
			});
		});

		// After bulk import succeeds with multiple meals, the page reloads meals via GET /api/meals.
		await page.route('**/api/meals', async (route) => {
			if (route.request().method() === 'GET') {
				await route.fulfill({
					status: 200,
					contentType: 'application/json',
					body: JSON.stringify([fakeMeal1, fakeMeal2]),
				});
			} else {
				await route.continue();
			}
		});

		await page.getByRole('button', { name: 'Import all' }).click();

		await expect(page.getByRole('dialog')).not.toBeVisible();
		await expect(page.getByRole('listitem').filter({ hasText: 'Imported Meal 1' })).toBeVisible();
		await expect(page.getByRole('listitem').filter({ hasText: 'Imported Meal 2' })).toBeVisible();
	});

	test('given_bulk_import_returns_failures_when_imported_then_results_shown', async ({ page }) => {
		await openImportUrls(page);

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

	test('given_51_urls_when_import_clicked_then_max_urls_error_and_no_request', async ({ page }) => {
		await openImportUrls(page);

		const urls = Array.from({ length: 51 }, (_, i) => `https://example.com/r${i + 1}`).join('\n');
		await page.getByPlaceholder('Paste recipe URLs, one per line…').fill(urls);

		let bulkCalled = false;
		await page.route('**/api/import/bulk', async (route) => {
			bulkCalled = true;
			await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ created: [], failed: [] }) });
		});

		// Button is disabled when >50 URLs; force-enable to trigger the handler
		await page.getByRole('button', { name: 'Import all' }).evaluate(el => (el as HTMLButtonElement).disabled = false);
		await page.getByRole('button', { name: 'Import all' }).click();

		await expect(page.getByRole('dialog')).toBeVisible();
		await expect(page.locator('.form-error')).toContainText('Maximum 50 URLs allowed');
		expect(bulkCalled).toBe(false);
	});
});
