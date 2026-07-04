import { expect, type APIRequestContext, type Page } from '@playwright/test';

const pagesWithInit = new WeakSet<Page>();

export async function setLocale(page: Page, locale: 'en' | 'de'): Promise<void> {
	if (pagesWithInit.has(page)) return;
	await page.addInitScript((l) => {
		if (!localStorage.getItem('yummybox-locale')) {
			localStorage.setItem('yummybox-locale', l);
		}
	}, locale);
}

export async function gotoEnglish(page: Page): Promise<void> {
	await setLocale(page, 'en');
	await page.goto('/meals');
}

export async function resetMeals(request: APIRequestContext): Promise<void> {
	const res = await request.get('/api/meals');
	if (!res.ok()) return;
	const meals = (await res.json()) as Array<{ id: number }>;
	await Promise.all(meals.map((m) => request.delete(`/api/meals/${m.id}`)));
}

export async function createMeal(
	page: Page,
	name: string,
	ingredients: Array<{ name: string; quantity?: string }>,
	instructions: string = 'Cooking steps'
): Promise<void> {
	// Navigate to meals page
	await page.goto('/meals');
	// Open the add meal modal
	await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
	// Wait for the modal dialog
	await expect(page.getByRole('dialog')).toBeVisible();
	// Meal name — use label association
	await page.getByLabel('Name', { exact: true }).fill(name);
	// Instructions are required by form validation
	await page.getByLabel('Instructions').fill(instructions);
	// Fill each ingredient row
	for (let i = 0; i < ingredients.length; i++) {
		const ing = ingredients[i];
		if (i > 0) {
			await page.getByRole('dialog').getByRole('button', { name: /^Add ingredient$|^Zutat hinzufügen$/ }).click();
		}
		const rowLabel = `Ingredient name ${i + 1}`;
		await page.getByRole('dialog').getByRole('textbox', { name: rowLabel }).fill(ing.name);
		if (ing.quantity) {
			// Quantity is a textbox with placeholder matching quantity
			const dialog = page.getByRole('dialog');
			await dialog.getByPlaceholder(/^Quantity$|^Menge$/).nth(i).fill(ing.quantity);
		}
	}
	await page.getByRole('dialog').getByRole('button', { name: /^(Add|Hinzufügen)$/ }).click();
	// Wait for modal to close
	await expect(page.getByRole('dialog')).not.toBeVisible();
	await expect(page.getByRole('listitem').filter({ hasText: name })).toBeVisible();
}

export async function createMealViaApi(
	request: APIRequestContext,
	name: string,
	ingredients: Array<{ name: string; quantity?: string }> = [{ name: 'flour' }],
	instructions = 'Cooking steps'
): Promise<{ id: number; name: string; ingredients: unknown; instructions: string; has_image: boolean }> {
	const response = await request.post('/api/meals', {
		multipart: {
			name,
			ingredients: JSON.stringify(ingredients),
			instructions,
		},
	});
	expect(response.ok()).toBe(true);
	return response.json();
}

export async function resetPlans(request: APIRequestContext): Promise<void> {
	const year = new Date().getUTCFullYear();
	const res = await request.get(`/api/plans?year=${year}`);
	if (!res.ok()) return;
	const summaries = (await res.json()) as Array<{ year: number; week_number: number }>;
	await Promise.all(summaries.map((p) => request.delete(`/api/plans/${p.year}/${p.week_number}`).catch(() => {})));
}
