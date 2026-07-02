import { test, expect } from '@playwright/test';
import { resetMeals, resetPlans, setLocale, createMealViaApi } from './_helpers';

test.describe('planner operations', () => {
	test.beforeEach(async ({ request, page }) => {
		await resetMeals(request);
		await resetPlans(request);
		await setLocale(page, 'en');
	});

	/** Click a specific week-cell row index (0=first visible, 5=last) and wait for generate form. */
	async function selectWeek(page, rowIndex: number) {
		await page.goto('/planner');
		await page.waitForSelector('.week-cell', { state: 'visible' });
		const cell = page.locator('.week-cell').nth(rowIndex);
		await cell.click();
		await page.waitForSelector('.plan-generate', { state: 'visible' });
		return rowIndex; // return index so caller can re-query
	}

	async function planMealCount(page): Promise<number> {
		return page.locator('.plan-meal-card:not(.plan-meal-card--add)').count();
	}

	async function generatePlan(page): Promise<void> {
		await page.getByRole('button', { name: 'Generate meal plan' }).click();
		await expect(page.locator('.plan-meal-grid')).toBeVisible({ timeout: 10000 });
	}

	// Each test uses a different cell index so they don't interfere with each other.
	// The calendar shows 6 rows (indices 0-5). We use later indices (future weeks).

	test('given_future_week_when_generate_plan_clicked_then_plan_created_with_meals', async ({ page, request }) => {
		await createMealViaApi(request, 'Pasta', [{ name: 'flour' }]);
		const idx = await selectWeek(page, 4); // second-to-last future week

		await generatePlan(page);

		await expect(page.locator('.plan-meal-card:not(.plan-meal-card--add)').first()).toBeVisible();
		const count = await planMealCount(page);
		expect(count).toBeGreaterThanOrEqual(1);

		// Re-query the cell at the same index — it should now have has-plan class.
		await expect(page.locator('.week-cell').nth(idx)).toHaveClass(/week-cell--has-plan/);
	});

	test('given_existing_plan_when_add_meal_clicked_then_meal_appears_in_plan', async ({ page, request }) => {
		await createMealViaApi(request, 'Pasta', [{ name: 'flour' }]);
		const idx = await selectWeek(page, 3); // third-to-last

		await generatePlan(page);
		await expect(page.locator('.week-cell').nth(idx)).toHaveClass(/week-cell--has-plan/);

		const beforeCount = await planMealCount(page);

		// Add a second meal for the picker.
		await createMealViaApi(request, 'Salad', [{ name: 'lettuce' }]);

		await page.locator('.plan-meal-card--add').click();
		const dialog = page.getByRole('dialog', { name: 'Pick meals' });
		await expect(dialog).toBeVisible();

		await dialog
			.locator('.meal-picker__item', { hasText: 'Salad' })
			.getByRole('button', { name: 'Add to plan' })
			.click();

		await dialog.getByRole('button', { name: 'Close' }).click();
		await expect(dialog).not.toBeVisible();

		await expect(page.locator('.plan-meal-grid .plan-meal-card:not(.plan-meal-card--add)', { hasText: 'Salad' })).toBeVisible();
		const afterCount = await planMealCount(page);
		expect(afterCount).toBe(beforeCount + 1);
	});

	test('given_plan_with_multiple_meals_when_remove_clicked_then_meal_removed', async ({ page, request }) => {
		await createMealViaApi(request, 'Pasta', [{ name: 'flour' }]);
		const idx = await selectWeek(page, 2); // fourth-to-last

		await generatePlan(page);
		await createMealViaApi(request, 'Salad', [{ name: 'lettuce' }]);

		await page.locator('.plan-meal-card--add').click();
		const dialog = page.getByRole('dialog', { name: 'Pick meals' });
		await expect(dialog).toBeVisible();
		await dialog
			.locator('.meal-picker__item', { hasText: 'Salad' })
			.getByRole('button', { name: 'Add to plan' })
			.click();
		await dialog.getByRole('button', { name: 'Close' }).click();
		await expect(dialog).not.toBeVisible();

		const beforeCount = await planMealCount(page);
		const firstItem = page.locator('.plan-meal-card:not(.plan-meal-card--add)').first();
		const removedName = await firstItem.locator('.plan-meal-card__name').textContent();

		await page.getByRole('button', { name: /^Remove / }).first().click({ force: true });

		// Wait for the meal count to actually decrease (the API call is async).
		await expect(async () => {
			const c = await planMealCount(page);
			expect(c).toBe(beforeCount - 1);
		}).toPass({ timeout: 5000 });

		if (removedName) {
			await expect(
				page.locator('.plan-meal-grid .plan-meal-card:not(.plan-meal-card--add)', { hasText: removedName })
			).toHaveCount(0);
		}
	});

	test('given_existing_plan_when_delete_confirmed_then_plan_removed', async ({ page, request }) => {
		await createMealViaApi(request, 'Pasta', [{ name: 'flour' }]);
		const idx = await selectWeek(page, 5); // last week

		await generatePlan(page);
		await expect(page.locator('.week-cell').nth(idx)).toHaveClass(/week-cell--has-plan/);

		await page.locator('.plan-actions').getByRole('button', { name: 'Delete meal plan' }).click();
		const alert = page.getByRole('alertdialog', { name: 'Delete meal plan' });
		await expect(alert).toBeVisible();

		await alert.getByRole('button', { name: 'Delete meal plan' }).click();

		await expect(alert).not.toBeVisible();
		await expect(page.locator('.plan-generate')).toBeVisible({ timeout: 10000 });
		await expect(page.locator('.week-cell').nth(idx)).not.toHaveClass(/week-cell--has-plan/);
	});
});
