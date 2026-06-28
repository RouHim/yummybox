import { test, expect } from '@playwright/test';
import { createMeal, resetMeals, setLocale } from './_helpers';

test.describe('Edit meal', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('pre-populates form with current values', async ({ page }) => {
		await createMeal(page, 'Pasta', [{ name: 'noodles' }, { name: 'sauce' }]);

		const item = page.getByRole('listitem').filter({ hasText: 'Pasta' });
		await item.hover();
		await item.getByRole('button', { name: 'Edit' }).click();

		await expect(page.getByRole('dialog').getByLabel('Name', { exact: true })).toHaveValue('Pasta');
		await expect(page.getByRole('dialog').getByRole('textbox', { name: 'Ingredient name 1' })).toHaveValue('noodles');
	});

	test('updates the meal and reflects change in the list', async ({ page }) => {
		await createMeal(page, 'Pasta', [{ name: 'noodles' }, { name: 'sauce' }]);

		const item = page.getByRole('listitem').filter({ hasText: 'Pasta' });
		await item.hover();
		await item.getByRole('button', { name: 'Edit' }).click();

		await page.getByRole('dialog').getByLabel('Name', { exact: true }).fill('Pasta Carbonara');
		await page.getByRole('dialog').getByRole('button', { name: /^(Save|Speichern)$/ }).click();

		await expect(page.getByRole('listitem').filter({ hasText: 'Pasta Carbonara' })).toBeVisible();
		await expect(page.getByText('Pasta', { exact: true })).not.toBeVisible();
	});
});
