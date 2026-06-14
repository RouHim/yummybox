import { test, expect } from '@playwright/test';
import { createMeal, resetMeals, setLocale } from './_helpers';

test.describe('Delete meal', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('accepting the confirmation removes the meal', async ({ page }) => {
		await page.goto('/');
		await createMeal(page, 'Soup', 'water, salt');

		page.once('dialog', (d) => d.accept());

		const item = page.getByRole('listitem').filter({ hasText: 'Soup' });
		await item.getByRole('button', { name: 'Delete' }).click();

		await expect(page.getByText('Soup')).not.toBeVisible();
	});

	test('dismissing the confirmation keeps the meal', async ({ page }) => {
		await page.goto('/');
		await createMeal(page, 'Soup', 'water, salt');

		page.once('dialog', (d) => d.dismiss());

		const item = page.getByRole('listitem').filter({ hasText: 'Soup' });
		await item.getByRole('button', { name: 'Delete' }).click();

		await expect(page.getByRole('listitem').filter({ hasText: 'Soup' })).toBeVisible();
	});
});
