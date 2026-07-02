import { test, expect } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

test.describe('Import meal from URL', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('given_valid_recipe_url_when_imported_then_form_populated_and_meal_added', async ({ page }) => {
		// Stub the backend import endpoint with a fixed draft
		await page.route('**/api/import/url', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					name: 'Test Curry',
					ingredients: [
						{ name: 'chicken', quantity: '250 g' },
						{ name: 'rice', quantity: '150 g' },
					],
					instructions: 'Cook the rice. Fry the chicken. Serve together.',
					imageBase64: null,
				}),
			});
		});

		await page.goto('/meals');
		// Open add-meal dialog
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		// Type the URL into the import textarea
		await page.getByRole('textbox', { name: 'Recipe URL or raw HTML/JSON-LD' }).fill('https://example.com/recipe');
		// Click Fetch & parse
		await page.getByRole('button', { name: /^Fetch & parse$/ }).click();
		// Wait for import to complete: import panel collapses, "Import another" button appears
		await expect(page.getByRole('button', { name: /^Import another$/ })).toBeVisible();
		// Verify Name field is populated (this was the bug — name was empty)
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Test Curry');
		// Verify ingredient name is populated
		await expect(page.getByRole('textbox', { name: 'Ingredient name 1' })).toHaveValue('chicken');
		// Verify instructions are populated
		await expect(page.getByLabel('Instructions')).toHaveValue('Cook the rice. Fry the chicken. Serve together.');
		// Click Add — this was the silent no-op before the fix
		await page.getByRole('dialog').getByRole('button', { name: /^(Add|Hinzufügen)$/ }).click();
		// Dialog closes
		await expect(page.getByRole('dialog')).not.toBeVisible();
		// Meal appears in the list
		await expect(page.getByRole('listitem').filter({ hasText: 'Test Curry' })).toBeVisible();
	});

	test('given_imported_meal_when_close_clicked_then_dialog_closes', async ({ page }) => {
		// This covers the second symptom: close button was dead after import (reactivity dead-locked)
		await page.route('**/api/import/url', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					name: 'Closeable Meal',
					ingredients: [{ name: 'flour', quantity: null }],
					instructions: 'Mix and bake.',
					imageBase64: null,
				}),
			});
		});

		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('textbox', { name: 'Recipe URL or raw HTML/JSON-LD' }).fill('https://example.com/recipe');
		await page.getByRole('button', { name: /^Fetch & parse$/ }).click();
		await expect(page.getByRole('button', { name: /^Import another$/ })).toBeVisible();
		// Click Close — this was broken before the fix (reactivity dead-locked)
		await page.getByRole('button', { name: /^Close$/ }).click();
		await expect(page.getByRole('dialog')).not.toBeVisible();
	});
});
