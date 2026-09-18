import { test, expect } from '@playwright/test';
import { resetMeals, setLocale, createMealViaApi } from './_helpers';

test.describe('Cooking view instructions rendering', () => {
	test.beforeEach(async ({ request }) => {
		await resetMeals(request);
	});

	test('renders sanitized HTML instructions as formatted paragraphs', async ({ page, request }) => {
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

test.describe('Cooking view viewport fit', () => {
	test.beforeEach(async ({ request }) => {
		await resetMeals(request);
	});

	// The long URL is what inflates the layout: its intrinsic width used to stretch the whole
	// card past the viewport. Meal images are laid out inside the card and never widen it.
	test('given a long source URL when opened on a narrow viewport then the page does not overflow horizontally', async ({ page, request }) => {
		await setLocale(page, 'en');

		const longSourceUrl =
			'https://www.hellofresh.de/recipes/hahnchen-curry-lauch-suppe-thermomix-605cf1b5d693c438fd650d96?isMegaAddonsEnabled=false&subscriptionId=1169483';
		const meal = await createMealViaApi(
			request,
			'Long Source URL Meal',
			[{ name: 'flour', quantity: '200 g' }],
			'Cook it',
			undefined,
			longSourceUrl,
		);

		await page.setViewportSize({ width: 390, height: 844 });
		await page.goto(`/meals/${meal.id}`);

		const card = page.locator('.cooking-view');
		await expect(card).toBeVisible();
		await expect(page.locator('.cooking-view__source-link')).toBeVisible();

		const metrics = await page.evaluate(() => {
			const box = document.querySelector('.cooking-view')!.getBoundingClientRect();
			return {
				documentScrollWidth: document.documentElement.scrollWidth,
				viewportWidth: document.documentElement.clientWidth,
				cardWidth: Math.round(box.width),
				cardRight: Math.round(box.right),
				bodyScrollWidth: document.querySelector('.cooking-view__body')!.scrollWidth,
			};
		});

		expect(metrics.documentScrollWidth).toBeLessThanOrEqual(metrics.viewportWidth);
		expect(metrics.cardRight).toBeLessThanOrEqual(metrics.viewportWidth);
		expect(metrics.bodyScrollWidth).toBeLessThanOrEqual(metrics.cardWidth);
	});
});
