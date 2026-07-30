import { test, expect } from '@playwright/test';
import { createMealViaApi, resetMeals, setLocale } from './_helpers';

test.describe('Portions', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('shows "Serves N" in cooking view when meal has portions', async ({ request, page }) => {
		const meal = await createMealViaApi(
			request,
			'Portioned Meal',
			[{ name: 'rice', quantity: '200g' }],
			'Cook rice',
			4,
		);

		await page.goto(`/meals/${meal.id}`);

		await expect(page.locator('.cooking-view__servings-label')).toContainText('Serves 4');
		await expect(page.locator('.cooking-view__stepper-value')).toContainText('4');
	});

	test('hides servings section when meal has no portions', async ({ request, page }) => {
		const meal = await createMealViaApi(
			request,
			'No Portions Meal',
			[{ name: 'egg' }],
			'Fry egg',
		);

		await page.goto(`/meals/${meal.id}`);

		await expect(page.locator('.cooking-view__servings')).toHaveCount(0);
	});

	test('increment stepper increases desired portions', async ({ request, page }) => {
		const meal = await createMealViaApi(
			request,
			'Scale Up Meal',
			[{ name: 'flour', quantity: '100g' }],
			'Mix flour',
			2,
		);

		await page.goto(`/meals/${meal.id}`);

		const stepperValue = page.locator('.cooking-view__stepper-value');
		await expect(stepperValue).toContainText('2');

		await page.getByRole('button', { name: 'More portions' }).click();
		await expect(stepperValue).toContainText('3');

		await page.getByRole('button', { name: 'More portions' }).click();
		await expect(stepperValue).toContainText('4');
	});

	test('decrement stepper decreases desired portions', async ({ request, page }) => {
		const meal = await createMealViaApi(
			request,
			'Scale Down Meal',
			[{ name: 'sugar', quantity: '300g' }],
			'Mix sugar',
			5,
		);

		await page.goto(`/meals/${meal.id}`);

		const stepperValue = page.locator('.cooking-view__stepper-value');
		await expect(stepperValue).toContainText('5');

		await page.getByRole('button', { name: 'Fewer portions' }).click();
		await expect(stepperValue).toContainText('4');

		await page.getByRole('button', { name: 'Fewer portions' }).click();
		await expect(stepperValue).toContainText('3');
	});

	test('decrement button is disabled at 1 portion', async ({ request, page }) => {
		const meal = await createMealViaApi(
			request,
			'Single Portion Meal',
			[{ name: 'butter', quantity: '50g' }],
			'Melt butter',
			1,
		);

		await page.goto(`/meals/${meal.id}`);

		await expect(page.locator('.cooking-view__stepper-value')).toContainText('1');
		await expect(page.getByRole('button', { name: 'Fewer portions' })).toBeDisabled();
	});

	test('increment button is disabled at 10000 portions', async ({ request, page }) => {
		const meal = await createMealViaApi(
			request,
			'Massive Meal',
			[{ name: 'salt' }],
			'Sprinkle salt',
			10000,
		);

		await page.goto(`/meals/${meal.id}`);

		await expect(page.locator('.cooking-view__stepper-value')).toContainText('10000');
		await expect(page.getByRole('button', { name: 'More portions' })).toBeDisabled();
	});

	test('scales ingredient quantities when desired portions differ', async ({ request, page }) => {
		const meal = await createMealViaApi(
			request,
			'Scalable Meal',
			[{ name: 'pasta', quantity: '200g' }, { name: 'cheese', quantity: '50g' }],
			'Cook pasta',
			2,
		);

		await page.goto(`/meals/${meal.id}`);

		// At 2 portions (no scaling yet), both original quantities visible
		await expect(page.locator('.cooking-view__qty').filter({ hasText: '200g' })).toBeVisible();
		await expect(page.locator('.cooking-view__qty').filter({ hasText: '50g' })).toBeVisible();

		// Scale up to 4 portions
		await page.getByRole('button', { name: 'More portions' }).click();
		await page.getByRole('button', { name: 'More portions' }).click();

		// Scaled quantities should appear: 200g * 4/2 = 400g, 50g * 4/2 = 100g
		await expect(page.locator('.cooking-view__qty--scaled').filter({ hasText: '400g' })).toBeVisible();
		await expect(page.locator('.cooking-view__qty--scaled').filter({ hasText: '100g' })).toBeVisible();

		// Original quantities still visible but muted
		await expect(page.locator('.cooking-view__qty--muted').filter({ hasText: '200g' })).toBeVisible();
		await expect(page.locator('.cooking-view__qty--muted').filter({ hasText: '50g' })).toBeVisible();
	});

	test('scaling resets to original when desired equals base portions', async ({ request, page }) => {
		const meal = await createMealViaApi(
			request,
			'Reset Scale Meal',
			[{ name: 'milk', quantity: '250ml' }],
			'Heat milk',
			3,
		);

		await page.goto(`/meals/${meal.id}`);

		// Scale up to 6
		await page.getByRole('button', { name: 'More portions' }).click(); // 4
		await page.getByRole('button', { name: 'More portions' }).click(); // 5
		await page.getByRole('button', { name: 'More portions' }).click(); // 6
		await expect(page.locator('.cooking-view__qty--scaled').filter({ hasText: '500ml' })).toBeVisible();

		// Scale back down to 3
		await page.getByRole('button', { name: 'Fewer portions' }).click(); // 5
		await page.getByRole('button', { name: 'Fewer portions' }).click(); // 4
		await page.getByRole('button', { name: 'Fewer portions' }).click(); // 3

		// Scaled quantities should disappear when back at base
		await expect(page.locator('.cooking-view__qty--scaled')).toHaveCount(0);
	});

	test('ingredient without quantity does not show scaled value', async ({ request, page }) => {
		const meal = await createMealViaApi(
			request,
			'No Qty Meal',
			[{ name: 'water' }, { name: 'salt', quantity: '1 pinch' }],
			'Boil water',
			2,
		);

		await page.goto(`/meals/${meal.id}`);

		// Scale up
		await page.getByRole('button', { name: 'More portions' }).click();
		await page.getByRole('button', { name: 'More portions' }).click(); // 4

		// Salt (has quantity) should scale
		await expect(page.locator('.cooking-view__qty--scaled').filter({ hasText: '2 pinch' })).toBeVisible();

		// Water (no quantity) should not have a scaling row
		const ingredientList = page.locator('.cooking-view__ingredient-list');
		// water has no qty span at all, only the name span
		const waterLi = ingredientList.getByRole('listitem').filter({ hasText: 'water' });
		await expect(waterLi.locator('.cooking-view__qty')).toHaveCount(0);
	});

	test('adding a meal via form with portions persists the value', async ({ page }) => {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();

		await page.getByLabel('Name', { exact: true }).fill('Form Portions Meal');
		await page.getByLabel('Portions').fill('6');
		await page.getByRole('dialog').getByRole('textbox', { name: 'Ingredient name 1' }).fill('chicken');
		await page.getByLabel('Instructions').fill('Grill chicken');

		await page.getByRole('dialog').getByRole('button', { name: /^Add$/ }).click();
		await expect(page.getByRole('dialog')).not.toBeVisible();

		// Navigate to cooking view and verify portions
		await page.getByRole('listitem').filter({ hasText: 'Form Portions Meal' }).click();
		await expect(page.locator('.cooking-view__servings-label')).toContainText('Serves 6');
	});

	test('rejects portions <= 0 via API', async ({ request }) => {
		const res = await request.post('/api/meals', {
			multipart: {
				name: 'Bad Portions',
				ingredients: JSON.stringify([{ name: 'x' }]),
				instructions: 'Test',
				portions: '0',
			},
		});
		expect(res.status()).toBe(400);
		const body = await res.json();
		expect(body.error).toContain('portions');
	});

	test('rejects portions > 10000 via API', async ({ request }) => {
		const res = await request.post('/api/meals', {
			multipart: {
				name: 'Huge Portions',
				ingredients: JSON.stringify([{ name: 'x' }]),
				instructions: 'Test',
				portions: '10001',
			},
		});
		expect(res.status()).toBe(400);
		const body = await res.json();
		expect(body.error).toContain('portions');
	});

	test('clearing portions field treats as no portions', async ({ page }) => {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();

		await page.getByLabel('Name', { exact: true }).fill('Cleared Portions');
		await page.getByLabel('Portions').fill('3');
		await page.getByLabel('Portions').clear();
		await page.getByRole('dialog').getByRole('textbox', { name: 'Ingredient name 1' }).fill('x');
		await page.getByLabel('Instructions').fill('Test');

		await page.getByRole('dialog').getByRole('button', { name: /^Add$/ }).click();
		await expect(page.getByRole('dialog')).not.toBeVisible();

		await page.getByRole('listitem').filter({ hasText: 'Cleared Portions' }).click();

		// No servings section should appear
		await expect(page.locator('.cooking-view__servings')).toHaveCount(0);
	});

	test('scaling handles fractional quantities correctly', async ({ request, page }) => {
		const meal = await createMealViaApi(
			request,
			'Fractional Meal',
			[{ name: 'oil', quantity: '1.5 tbsp' }],
			'Drizzle oil',
			2,
		);

		await page.goto(`/meals/${meal.id}`);

		// Scale to 3 portions: 1.5 * 3/2 = 2.25 → toFixed(1) → "2.3"
		await page.getByRole('button', { name: 'More portions' }).click();

		await expect(page.locator('.cooking-view__qty--scaled').filter({ hasText: '2.3 tbsp' })).toBeVisible();
	});
});
