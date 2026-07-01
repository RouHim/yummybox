import { test, expect } from '@playwright/test';
import { resetMeals } from './_helpers';

test.describe('planner', () => {
	test.beforeEach(async ({ request }) => {
		await resetMeals(request);
	});

	test('given_no_plan_exists_when_clicking_future_week_then_generate_form_shown_with_no_error', async ({ page }) => {
		await page.goto('/planner');

		// Wait for the week grid to render
		await page.waitForSelector('.week-cell', { state: 'visible' });

		// Find a week cell without has-plan class and click it
		const cells = page.locator('.week-cell:not(.week-cell--has-plan)');
		const count = await cells.count();
		if (count > 0) {
			await cells.last().click();
		}
		// Wait for the generate form to appear
		await page.waitForSelector('.plan-generate', { state: 'visible' });
		await expect(page.locator('.form-error')).toHaveCount(0);
		await expect(page.getByRole('spinbutton', { name: 'Number of meals' })).toBeVisible();
	});

	test('given_past_weeks_in_current_year_when_planner_loads_then_past_cells_have_muted_class', async ({ page }) => {
		await page.goto('/planner');

		// Wait for the week grid to render
		await page.waitForSelector('.week-cell', { state: 'visible' });

		// Weeks before current week should have the past class
		const pastCells = page.locator('.week-cell--past');
		await expect(pastCells.first()).toBeVisible();

		// Future weeks should not have past class
		const futureCells = page.locator('.week-cell:not(.week-cell--past)');
		await expect(futureCells.first()).toBeVisible();
	});

	test('given_past_year_when_planner_loads_then_all_weeks_have_muted_class', async ({ page }) => {
		await page.goto('/planner');

		// Wait for the week grid to render
		await page.waitForSelector('.week-cell', { state: 'visible' });

		// Navigate back to a fully-past year by stepping months
		for (let i = 0; i < 12; i++) {
			await page.getByRole('button', { name: 'Previous month' }).click();
		}

		// All visible week cells should be past
		const totalCells = await page.locator('.week-cell').count();
		const pastCells = await page.locator('.week-cell--past').count();
		expect(pastCells).toBe(totalCells);
	});

	test('given_no_plan_exists_when_generate_form_appears_then_meal_count_defaults_to_3', async ({ page }) => {
		await page.goto('/planner');

		// Wait for the week grid to render
		await page.waitForSelector('.week-cell', { state: 'visible' });

		// Click a week cell without a plan
		const cell = page.locator('.week-cell:not(.week-cell--has-plan)').first();
		await cell.click();

		// Wait for the generate form
		await page.waitForSelector('.plan-generate', { state: 'visible' });
		const input = page.locator('input.plan-count-input');
		await expect(input).toHaveValue('3');
	});

	test('given_user_changes_meal_count_when_clicking_new_week_then_meal_count_resets_to_3', async ({ page }) => {
		await page.goto('/planner');

		// Wait for the week grid to render
		await page.waitForSelector('.week-cell', { state: 'visible' });

		// Click a no-plan week, change count, click another no-plan week
		const cells = page.locator('.week-cell:not(.week-cell--has-plan)');
		const count = await cells.count();
		if (count < 2) {
			test.skip(true, 'need at least two weeks without plans');
			return;
		}

		await cells.nth(0).click();
		await page.waitForSelector('.plan-generate', { state: 'visible' });
		const input = page.locator('input.plan-count-input');
		await input.fill('7');
		await cells.nth(1).click();
		await page.waitForSelector('.plan-generate', { state: 'visible' });
		await expect(input).toHaveValue('3');
	});

	test('given_no_toggle_exists_when_rendering_planner_then_no_lang_toggle_visible', async ({ page }) => {
		await page.goto('/planner');
		await expect(page.locator('.lang-toggle')).toHaveCount(0);
	});

	test('given_no_toggle_exists_when_rendering_home_then_no_lang_toggle_visible', async ({ page }) => {
		await page.goto('/');
		await expect(page.locator('.lang-toggle')).toHaveCount(0);
	});

	test('given_navigator_de_DE_when_loading_app_then_mealme_locale_localStorage_is_not_written', async ({ browser }) => {
		const context = await browser.newContext({ locale: 'de-DE' });
		const page = await context.newPage();
		await page.goto('/');
		const stored = await page.evaluate(() => localStorage.getItem('mealme.locale'));
		expect(stored).toBeNull();
		await context.close();
	});

	test('given_navigator_en_US_when_loading_app_then_mealme_locale_localStorage_is_not_written', async ({ browser }) => {
		const context = await browser.newContext({ locale: 'en-US' });
		const page = await context.newPage();
		await page.goto('/');
		const stored = await page.evaluate(() => localStorage.getItem('mealme.locale'));
		expect(stored).toBeNull();
		await context.close();
	});
});
