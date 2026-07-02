import { test, expect } from '@playwright/test';
import { resetMeals, resetPlans, setLocale, createMealViaApi } from './_helpers';

test.describe('Bring! integration', () => {
	test.beforeEach(async ({ request, page }) => {
		await resetMeals(request);
		await resetPlans(request);
		await setLocale(page, 'en');
	});

	test('given_bring_configured_when_ingredient_sent_then_success_shown', async ({ page, request }) => {
		await createMealViaApi(request, 'Pasta', [{ name: 'flour', quantity: '200 g' }]);

		await page.route('**/api/bring/status', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ configured: true, connected: true, error: null }),
			});
		});

		await page.goto('/planner');

		// Generate a plan: pick the last week row (guaranteed empty after resetPlans).
		await page.goto('/planner');
		await page.waitForSelector('.week-cell', { state: 'visible' });
		await page.locator('.week-cell').nth(5).click();
		await page.waitForSelector('.plan-generate', { state: 'visible' });

		await page.getByRole('button', { name: 'Generate meal plan' }).click();
		await page.waitForSelector('.plan-summary', { state: 'visible' });

		const flourButton = page.locator('.summary-card', { hasText: 'flour' }).getByRole('button');
		await expect(flourButton).toHaveAttribute('aria-label', 'Send to Bring!');

		await page.route('**/api/bring/items', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ sent: true }),
			});
		});

		await flourButton.click();

		await expect(flourButton).toBeDisabled();
		await expect(flourButton).toHaveAttribute('aria-label', 'Sent!');
	});

	test('given_bring_not_configured_when_app_loads_then_no_error_message_shown', async ({ page }) => {
		await page.route('**/api/bring/status', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ configured: false, connected: false, error: null }),
			});
		});

		await page.goto('/meals');
		await expect(page.locator('.site-footer__bring-error')).toHaveCount(0);
	});

	test('given_bring_error_state_when_app_loads_then_error_message_shown', async ({ page }) => {
		await page.route('**/api/bring/status', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ configured: true, connected: false, error: 'Auth failed' }),
			});
		});

		await page.goto('/meals');
		await expect(page.locator('.site-footer__bring-error[role="alert"]')).toContainText('Auth failed');
	});
});
