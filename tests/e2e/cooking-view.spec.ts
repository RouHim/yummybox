import { test, expect } from '@playwright/test';
import { resetMeals, setLocale, createMealViaApi } from './_helpers';

test.describe('Cooking view instructions rendering', () => {
	test('renders sanitized HTML instructions as formatted paragraphs', async ({ page, request }) => {
		await resetMeals(request);
		await setLocale(page, 'en');

		const meal = await createMealViaApi(
			request,
			'HTML Meal',
			[{ name: 'flour' }],
			'<p>Step 1</p><p>Step 2</p>',
		);

		await page.goto(`/meals/${meal.id}`);

		const container = page.locator('.cooking-view__instructions-text');
		await expect(container.locator('p')).toHaveCount(2);
		await expect(container).toContainText('Step 1');
		await expect(container).toContainText('Step 2');
		// No raw <p> text visible
		await expect(container.getByText('<p>')).toHaveCount(0);
	});

	test('renders plain text instructions with preserved newlines', async ({ page, request }) => {
		await resetMeals(request);
		await setLocale(page, 'en');

		const meal = await createMealViaApi(
			request,
			'Plain Text Meal',
			[{ name: 'egg' }],
			'Step 1\nStep 2\nStep 3',
		);

		await page.goto(`/meals/${meal.id}`);

		const container = page.locator('.cooking-view__instructions-text');
		const whiteSpace = await container.evaluate(el => getComputedStyle(el).whiteSpace);
		expect(whiteSpace).toBe('pre-wrap');
		const text = await container.textContent();
		expect(text).toContain('Step 1');
		expect(text).toContain('Step 2');
		expect(text).toContain('Step 3');
	});
});
