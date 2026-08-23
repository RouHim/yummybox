import { test, expect } from '@playwright/test';
import { resetMeals, resetPlans, setLocale, createMealViaApi } from './_helpers';

test.describe('network errors', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
		await resetPlans(request);
	});

	test('given_network_failure_when_saving_meal_then_shows_error_and_keeps_input', async ({ page, request }) => {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByLabel('Name', { exact: true }).fill('Unicorn');
		await page.getByRole('dialog').getByRole('textbox', { name: 'Ingredient name 1' }).fill('sparkle');
		await page.getByLabel('Instructions').fill('Mix');
		// Abort only the POST — the page's initial GET already happened.
		await page.route('**/api/meals', (route) =>
			route.request().method() === 'POST' ? route.abort() : route.continue()
		);
		await page.getByRole('dialog').getByRole('button', { name: /^(Add|Hinzufügen)$/ }).click();
		await expect(page.getByRole('alert')).toContainText('Failed to save meal');
		await expect(page.getByRole('dialog')).toBeVisible();
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Unicorn');
	});

	test('given_network_failure_when_adding_meal_to_plan_then_shows_error_and_keeps_plan', async ({ page, request }) => {
		await createMealViaApi(request, 'Pasta', [{ name: 'flour' }]);
		await page.goto('/planner');
		await page.waitForSelector('.week-cell', { state: 'visible' });
		await page.locator('.week-cell').nth(4).click();
		await page.waitForSelector('.plan-generate', { state: 'visible' });
		await page.getByRole('button', { name: 'Generate meal plan' }).click();
		await expect(page.locator('.plan-meal-grid')).toBeVisible({ timeout: 10000 });
		await createMealViaApi(request, 'Salad', [{ name: 'lettuce' }]);
		// Abort only PUTs — the generate POST already completed.
		await page.route('**/api/plans/**', (route) =>
			route.request().method() === 'PUT' ? route.abort() : route.continue()
		);
		await page.locator('.plan-meal-card--add').click();
		const dialog = page.getByRole('dialog', { name: 'Pick meals' });
		await expect(dialog).toBeVisible();
		await dialog
			.locator('.meal-picker__item', { hasText: 'Salad' })
			.getByRole('button', { name: 'Add to plan' })
			.click();
		await expect(page.getByRole('alert')).toContainText('Could not reach the server');
		await dialog.getByRole('button', { name: 'Close' }).click();
		await expect(
			page.locator('.plan-meal-grid .plan-meal-card:not(.plan-meal-card--add)', { hasText: 'Pasta' })
		).toBeVisible();
	});
});
