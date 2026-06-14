import { test, expect } from '@playwright/test';
import { createMeal, resetMeals, setLocale } from './_helpers';

test.describe('View meals', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('shows empty state when no meals exist', async ({ page }) => {
		await page.goto('/');
		await expect(page.getByText('No meals yet. Add your first one.')).toBeVisible();
	});

	test('shows meal name and ingredients preview after a meal is added', async ({ page }) => {
		await page.goto('/');
		await createMeal(page, 'Pasta', 'noodles, sauce');

		const item = page.getByRole('listitem').filter({ hasText: 'Pasta' });
		await expect(item).toBeVisible();
		await expect(item).toContainText('noodles, sauce');
	});
});
