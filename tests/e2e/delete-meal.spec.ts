import { test, expect } from '@playwright/test';
import { createMeal, resetMeals, setLocale } from './_helpers';

test.describe('Delete meal', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('accepting the confirmation removes the meal', async ({ page }) => {
		await createMeal(page, 'Soup', [{ name: 'water' }, { name: 'salt' }]);

		const item = page.getByRole('listitem').filter({ hasText: 'Soup' });
		await item.hover();
		await item.getByRole('button', { name: 'Delete' }).click();

		await expect(page.getByRole('alertdialog')).toBeVisible();
		await page.getByRole('alertdialog').getByRole('button', { name: /^(Delete|Löschen)$/ }).click();
		await expect(page.getByRole('alertdialog')).not.toBeVisible();

		await expect(page.getByText('Soup')).not.toBeVisible();
	});

	test('dismissing the confirmation keeps the meal', async ({ page }) => {
		await createMeal(page, 'Soup', [{ name: 'water' }, { name: 'salt' }]);

		const item = page.getByRole('listitem').filter({ hasText: 'Soup' });
		await item.hover();
		await item.getByRole('button', { name: 'Delete' }).click();

		await expect(page.getByRole('alertdialog')).toBeVisible();
		await page.getByRole('alertdialog').getByRole('button', { name: /^(Cancel|Abbrechen)$/ }).click();
		await expect(page.getByRole('alertdialog')).not.toBeVisible();

		await expect(page.getByRole('listitem').filter({ hasText: 'Soup' })).toBeVisible();
	});

	test('pressing Escape dismisses the confirmation', async ({ page }) => {
		await createMeal(page, 'Soup', [{ name: 'water' }, { name: 'salt' }]);

		const item = page.getByRole('listitem').filter({ hasText: 'Soup' });
		await item.hover();
		await item.getByRole('button', { name: 'Delete' }).click();

		await expect(page.getByRole('alertdialog')).toBeVisible();
		await page.keyboard.press('Escape');
		await expect(page.getByRole('alertdialog')).not.toBeVisible();

		await expect(page.getByRole('listitem').filter({ hasText: 'Soup' })).toBeVisible();
	});
});
