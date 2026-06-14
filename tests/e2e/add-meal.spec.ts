import { test, expect } from '@playwright/test';
import { createMeal, resetMeals, setLocale } from './_helpers';

test.describe('Add meal', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('adds a valid meal and shows it in the list', async ({ page }) => {
		await page.goto('/');
		await createMeal(page, 'Salad', 'lettuce, tomato');
		await expect(page.getByRole('listitem').filter({ hasText: 'Salad' })).toBeVisible();
	});

	test('shows validation error for empty name', async ({ page }) => {
		await page.goto('/');
		await page.getByLabel('Ingredients').fill('x');
		await page.getByRole('button', { name: 'Add' }).click();
		await expect(page.getByText('Name is required')).toBeVisible();
	});

	test('shows validation error for empty ingredients', async ({ page }) => {
		await page.goto('/');
		await page.getByLabel('Name').fill('x');
		await page.getByRole('button', { name: 'Add' }).click();
		await expect(page.getByText('Ingredients are required')).toBeVisible();
	});
});
