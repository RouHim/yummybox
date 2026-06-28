import { test, expect } from '@playwright/test';
import { createMeal, resetMeals, setLocale } from './_helpers';

test.describe('Add meal', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('adds a valid meal and shows it in the list', async ({ page }) => {
		await createMeal(page, 'Salad', [{ name: 'lettuce' }, { name: 'tomato' }]);
		await expect(page.getByRole('listitem').filter({ hasText: 'Salad' })).toBeVisible();
	});

	test('shows validation error for empty name', async ({ page }) => {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		// Fill ingredient name but leave meal name empty
		await page.getByRole('dialog').getByRole('textbox', { name: 'Ingredient name 1' }).fill('x');
		// Click the form submit button (regex to avoid "Add ingredient" button)
		await page.getByRole('dialog').getByRole('button', { name: /^(Add|Hinzufügen)$/ }).click();
		await expect(page.getByText('Name is required')).toBeVisible();
	});

	test('shows validation error for empty ingredients', async ({ page }) => {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('dialog').getByLabel('Name', { exact: true }).fill('x');
		// Leave ingredient row empty, click submit
		await page.getByRole('dialog').getByRole('button', { name: /^(Add|Hinzufügen)$/ }).click();
		await expect(page.getByText('At least one ingredient is required')).toBeVisible();
	});
});
