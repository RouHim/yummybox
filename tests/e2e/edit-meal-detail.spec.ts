import { test, expect } from '@playwright/test';
import { createMealViaApi, resetMeals, setLocale } from './_helpers';

test.describe('Edit meal from detail page', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('given_meal_detail_view_when_edited_then_stays_on_detail_page', async ({ page }) => {
		const meal = await createMealViaApi(page.request, 'Pancakes', [{ name: 'flour' }, { name: 'milk' }]);

		await page.goto(`/meals/${meal.id}`);

		// Click the edit button in the hero overlay
		await page.getByRole('button', { name: /^Edit$|^Bearbeiten$/ }).click({ force: true });

		const dialog = page.getByRole('dialog');
		await expect(dialog).toBeVisible();

		// Change an ingredient
		await dialog.getByRole('textbox', { name: 'Ingredient name 1' }).clear();
		await dialog.getByRole('textbox', { name: 'Ingredient name 1' }).fill('whole wheat flour');

		await dialog.getByRole('button', { name: /^(Save|Speichern)$/ }).click();

		// Dialog closes
		await expect(dialog).not.toBeVisible();

		// URL must stay on the detail page
		await expect(page).toHaveURL(`/meals/${meal.id}`);

		// Updated ingredient must be visible
		await expect(page.locator('.cooking-view__ingredient-list')).toContainText('whole wheat flour');
	});
});
