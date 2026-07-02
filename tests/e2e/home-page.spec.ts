import { test, expect } from '@playwright/test';
import { resetMeals, setLocale, createMealViaApi } from './_helpers';

/** ISO week-of-year (Monday-start, week 1 contains Jan 4). Matches the backend's weekOfDate. */
function currentWeek(): { year: number; week: number } {
	const d = new Date();
	const year = d.getUTCFullYear();
	const jan1 = new Date(Date.UTC(year, 0, 1, 12, 0, 0));
	const day = jan1.getUTCDay();
	const daysFromMon = day === 0 ? 6 : day - 1;
	const mon = new Date(Date.UTC(year, 0, 1 - daysFromMon, 12, 0, 0));
	const diffDays = Math.floor((d.getTime() - mon.getTime()) / 86400000);
	return { year, week: Math.floor(diffDays / 7) + 1 };
}

test.describe('Home page current-week view', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('given_no_plan_for_current_week_when_home_loaded_then_redirects_to_planner', async ({ page }) => {
		await page.route('**/api/plans?*', async (route) => {
			await route.fulfill({ status: 404, contentType: 'application/json', body: JSON.stringify({ error: 'Plan not found' }) });
		});

		await page.goto('/');

		await expect(page).toHaveURL(/\/planner/);
		await expect(page.locator('.cal-grid')).toBeVisible();
	});

	test('given_plan_exists_for_current_week_when_home_loaded_then_meals_shown', async ({ page, request }) => {
		await createMealViaApi(request, 'Pasta', [{ name: 'flour' }]);
		await createMealViaApi(request, 'Salad', [{ name: 'lettuce' }]);

		// Create a plan for the current week via direct API call.
		const { year, week } = currentWeek();
		// Delete any existing plan first to avoid conflict.
		await request.delete(`/api/plans/${year}/${week}`).catch(() => {});
		await request.post('/api/plans', {
			data: { year, week_number: week, meal_count: 3 },
		});

		await page.goto('/');
		await expect(page.getByRole('heading', { name: 'Meals this week' })).toBeVisible();
		// Meal cards on home page are links with aria-label "Cook {name}"
		const cookLinks = page.getByRole('link', { name: /^Cook / });
		await expect(cookLinks.first()).toBeVisible();
	});

	test('given_empty_plan_for_current_week_when_home_loaded_then_empty_state_shown', async ({ page }) => {
		const { year, week } = currentWeek();

		await page.route('**/api/plans?*', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					id: 1,
					year,
					week_number: week,
					created_at: '2026-01-01T00:00:00Z',
					meals: [],
					ingredient_summary: [],
				}),
			});
		});

		await page.goto('/');
		await expect(page.getByText('No meals planned this week')).toBeVisible();
		await expect(page.getByRole('link', { name: 'Go to planner' })).toBeVisible();
	});
});
