import { test, expect } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

test.describe('theme toggle', () => {
	test.beforeEach(async ({ request, page }) => {
		await resetMeals(request);
		await setLocale(page, 'en');
	});

	test('given_default_theme_when_cycled_three_times_then_returns_to_system', async ({ page }) => {
		await page.goto('/meals');

		await page.evaluate(() => localStorage.removeItem('mealme-theme'));
		await page.goto('/meals');

		await expect(page.locator('html')).not.toHaveAttribute('data-theme');

		await page.getByRole('button', { name: 'Theme' }).click();
		await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
		expect(await page.evaluate(() => localStorage.getItem('mealme-theme'))).toBe('light');

		await page.getByRole('button', { name: 'Theme' }).click();
		await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

		await page.getByRole('button', { name: 'Theme' }).click();
		await expect(page.locator('html')).not.toHaveAttribute('data-theme');
		expect(await page.evaluate(() => localStorage.getItem('mealme-theme'))).toBe('system');
	});
});
