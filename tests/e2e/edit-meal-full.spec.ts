import { test, expect } from '@playwright/test';
import { createMealViaApi, resetMeals, setLocale } from './_helpers';

test.describe('Edit meal full', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('given_existing_meal_when_ingredients_edited_then_updated_in_list', async ({ page }) => {
		await createMealViaApi(page.request, 'Pasta', [{ name: 'flour', quantity: '100 g' }]);
		await page.goto('/meals');

		const item = page.getByRole('listitem').filter({ hasText: 'Pasta' });
		await item.hover();
		await item.getByRole('button', { name: /^Edit$|^Bearbeiten$/ }).click();

		const dialog = page.getByRole('dialog');
		await expect(dialog).toBeVisible();

		await page.getByRole('textbox', { name: 'Ingredient name 1' }).clear();
		await page.getByRole('textbox', { name: 'Ingredient name 1' }).fill('rice');

		await dialog.getByRole('button', { name: /^Add ingredient$|^Zutat hinzufügen$/ }).click();
		await page.getByRole('textbox', { name: 'Ingredient name 2' }).fill('sauce');

		await dialog.getByRole('button', { name: /^(Save|Speichern)$/ }).click();

		await expect(dialog).not.toBeVisible();
		const updatedItem = page.getByRole('listitem').filter({ hasText: 'Pasta' });
		await expect(updatedItem).toContainText('rice');
		await expect(updatedItem).toContainText('sauce');
	});

	test('given_existing_meal_when_instructions_edited_then_updated_in_cooking_view', async ({ page }) => {
		const meal = await createMealViaApi(page.request, 'Soup', [{ name: 'water' }], 'Old instructions');
		await page.goto('/meals');

		const item = page.getByRole('listitem').filter({ hasText: 'Soup' });
		await item.hover();
		await item.getByRole('button', { name: /^Edit$|^Bearbeiten$/ }).click();

		const dialog = page.getByRole('dialog');
		await expect(dialog).toBeVisible();

		await page.getByLabel('Instructions').clear();
		await page.getByLabel('Instructions').fill('New boiling steps');

		await dialog.getByRole('button', { name: /^(Save|Speichern)$/ }).click();

		await expect(dialog).not.toBeVisible();

		// Explicitly navigate to the cooking view to verify persisted instructions.
		await page.goto(`/meals/${meal.id}`);
		await expect(page.locator('.cooking-view__instructions-text')).toContainText('New boiling steps');
	});
});
