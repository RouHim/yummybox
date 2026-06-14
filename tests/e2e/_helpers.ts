import { expect, type APIRequestContext, type Page } from '@playwright/test';

const pagesWithInit = new WeakSet<Page>();

export async function setLocale(page: Page, locale: 'en' | 'de'): Promise<void> {
	if (pagesWithInit.has(page)) return;
	await page.addInitScript((l) => {
		if (!localStorage.getItem('mealme.locale')) {
			localStorage.setItem('mealme.locale', l);
		}
	}, locale);
}

export async function gotoEnglish(page: Page): Promise<void> {
	await setLocale(page, 'en');
	await page.goto('/');
}

export async function resetMeals(request: APIRequestContext): Promise<void> {
	const res = await request.get('/api/meals');
	if (!res.ok()) return;
	const meals = (await res.json()) as Array<{ id: number }>;
	await Promise.all(meals.map((m) => request.delete(`/api/meals/${m.id}`)));
}

export async function createMeal(page: Page, name: string, ingredients: string): Promise<void> {
	await page.getByLabel('Name').fill(name);
	await page.getByLabel('Ingredients').fill(ingredients);
	await page.getByRole('button', { name: /^(Add|Hinzufügen)$/ }).click();
	await expect(page.getByRole('listitem').filter({ hasText: name })).toBeVisible();
}
