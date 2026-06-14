import { test, expect } from '@playwright/test';
import { createMeal, resetMeals, setLocale } from './_helpers';

test.describe('Edit meal', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('pre-populates form with current values', async ({ page }) => {
		await page.goto('/');
		await createMeal(page, 'Pasta', 'noodles, sauce');

		const item = page.getByRole('listitem').filter({ hasText: 'Pasta' });
		await item.getByRole('button', { name: 'Edit' }).click();

		await expect(page.getByLabel('Name')).toHaveValue('Pasta');
		await expect(page.getByLabel('Ingredients')).toHaveValue('noodles, sauce');
	});

	test('updates the meal and reflects change in the list', async ({ page }) => {
		await page.goto('/');
		await createMeal(page, 'Pasta', 'noodles, sauce');

		const item = page.getByRole('listitem').filter({ hasText: 'Pasta' });
		await item.getByRole('button', { name: 'Edit' }).click();

		await page.getByLabel('Name').fill('Pasta Carbonara');
		await page.getByRole('button', { name: 'Save' }).click();

		await expect(page.getByRole('listitem').filter({ hasText: 'Pasta Carbonara' })).toBeVisible();
		await expect(page.getByText('Pasta', { exact: true })).not.toBeVisible();
	});
});
