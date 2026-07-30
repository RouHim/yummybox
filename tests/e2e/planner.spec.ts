import { test, expect } from '@playwright/test';
import { createMealViaApi, resetMeals, resetPlans } from './_helpers';
import { buildPng } from './_png';

test.describe('planner', () => {
	test.beforeEach(async ({ request }) => {
		await resetMeals(request);
		await resetPlans(request);
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

	test('given_no_week_selected_when_planner_loads_then_empty_state_shown_with_week_selector', async ({ page }) => {
		await page.goto('/planner');

		// Wait for the week grid to render
		await page.waitForSelector('.week-cell', { state: 'visible' });

		// Empty state should be visible (no week selected)
		const emptyState = page.locator('.planner-empty');
		await expect(emptyState).toBeVisible();

		// Title and subtitle should be shown
		await expect(emptyState.locator('h2')).toBeVisible();
		await expect(emptyState.locator('p')).toBeVisible();

		// Plan detail panel should NOT be visible
		await expect(page.locator('.plan-detail')).toHaveCount(0);

		// Click a week cell without a plan
		const cells = page.locator('.week-cell:not(.week-cell--has-plan)');
		const count = await cells.count();
		if (count > 0) {
			await cells.last().click();
		}

		// Empty state should be gone, generate form should appear
		await expect(emptyState).toHaveCount(0);
		await page.waitForSelector('.plan-generate', { state: 'visible' });
	});

	test('given_past_weeks_in_current_year_when_planner_loads_then_past_cells_have_muted_class', async ({ page }) => {
		// Pin clock to mid-March to guarantee past weeks exist in the month grid
		await page.clock.setFixedTime(new Date('2026-03-15T12:00:00Z'));

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

	test('given_language_switcher_when_rendering_planner_then_dropdown_visible', async ({ page }) => {
		await page.goto('/planner');
		await expect(page.locator('.lang-switcher')).toBeVisible();
	});

	test('given_language_switcher_when_rendering_home_then_dropdown_visible', async ({ page }) => {
		await page.goto('/');
		await expect(page.locator('.lang-switcher')).toBeVisible();
	});

	test('given_navigator_de_DE_when_loading_app_then_yummybox_locale_not_written', async ({ browser }) => {
		const context = await browser.newContext({ locale: 'de-DE' });
		const page = await context.newPage();
		await page.goto('/');
		const stored = await page.evaluate(() => localStorage.getItem('yummybox-locale'));
		expect(stored).toBeNull();
		await context.close();
	});

	test('given_navigator_en_US_when_loading_app_then_yummybox_locale_not_written', async ({ browser }) => {
		const context = await browser.newContext({ locale: 'en-US' });
		const page = await context.newPage();
		await page.goto('/');
		const stored = await page.evaluate(() => localStorage.getItem('yummybox-locale'));
		expect(stored).toBeNull();
		await context.close();
	});

	test('given_meal_with_image_when_opening_picker_then_thumbnail_shown_and_placeholder_for_no_image', async ({ page, request }) => {
		// Create a meal with an image via API multipart upload
		const png = buildPng(2, 2);
		const imgRes = await request.post('/api/meals', {
			multipart: {
				name: 'Photo Pasta',
				ingredients: JSON.stringify([{ name: 'noodles' }]),
				instructions: 'Boil and serve.',
				image: {
					name: 'photo.png',
					mimeType: 'image/png',
					buffer: png,
				},
			},
		});
		expect(imgRes.ok()).toBe(true);

		// Create meals without images
		await createMealViaApi(request, 'Plain Rice', [{ name: 'rice' }]);
		await createMealViaApi(request, 'Simple Soup', [{ name: 'broth' }]);

		await page.goto('/planner');
		await page.waitForSelector('.week-cell', { state: 'visible' });

		// Select a future week
		const cells = page.locator('.week-cell:not(.week-cell--has-plan)');
		const count = await cells.count();
		expect(count).toBeGreaterThan(0);
		await cells.last().click();

		// Generate a plan
		await page.waitForSelector('.plan-generate', { state: 'visible' });
		await page.locator('.plan-generate .btn--primary').click();

		// Wait for the "Add meal" button to appear in the plan grid
		await page.waitForSelector('.plan-meal-card--add', { state: 'visible' });

		// Open the meal picker overlay
		await page.locator('.plan-meal-card--add').click();
		await page.waitForSelector('.meal-picker', { state: 'visible' });

		// Meal with image: thumbnail <img> should be visible
		const photoItem = page.locator('.meal-picker__item').filter({ hasText: 'Photo Pasta' });
		await expect(photoItem.locator('.meal-picker__thumb-img')).toBeVisible();

		// Meal without image: no <img>, placeholder icon visible
		const plainItem = page.locator('.meal-picker__item').filter({ hasText: 'Plain Rice' });
		await expect(plainItem.locator('.meal-picker__thumb-img')).not.toBeAttached();
		await expect(plainItem.locator('.meal-picker__thumb-placeholder')).toBeVisible();

		// Wait for a visible thumbnail image (lazy-loaded)
		await photoItem.locator('.meal-picker__thumb-img').waitFor({ state: 'visible', timeout: 5000 });
	});
});
