import { test, expect } from '@playwright/test';
import { createMeal, resetMeals, setLocale } from './_helpers';

test.describe('Search meals', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('filters by meal name (case-insensitive)', async ({ page }) => {
		await page.goto('/');
		await createMeal(page, 'Pasta', 'noodles');
		await createMeal(page, 'Salad', 'lettuce');

		await page.getByPlaceholder('Search meals...').fill('pas');

		await expect(page.getByRole('listitem').filter({ hasText: 'Pasta' })).toBeVisible();
		await expect(page.getByRole('listitem').filter({ hasText: 'Salad' })).not.toBeVisible();
	});

	test('filters by ingredient', async ({ page }) => {
		await page.goto('/');
		await createMeal(page, 'Smoothie', 'banana, milk');
		await createMeal(page, 'Toast', 'bread');

		await page.getByPlaceholder('Search meals...').fill('banana');

		await expect(page.getByRole('listitem').filter({ hasText: 'Smoothie' })).toBeVisible();
		await expect(page.getByRole('listitem').filter({ hasText: 'Toast' })).not.toBeVisible();
	});

	test('clearing the search shows all meals again', async ({ page }) => {
		await page.goto('/');
		await createMeal(page, 'Smoothie', 'banana, milk');
		await createMeal(page, 'Toast', 'bread');

		await page.getByPlaceholder('Search meals...').fill('banana');
		await expect(page.getByRole('listitem').filter({ hasText: 'Toast' })).not.toBeVisible();

		await page.getByPlaceholder('Search meals...').fill('');
		await expect(page.getByRole('listitem').filter({ hasText: 'Smoothie' })).toBeVisible();
		await expect(page.getByRole('listitem').filter({ hasText: 'Toast' })).toBeVisible();
	});
});
