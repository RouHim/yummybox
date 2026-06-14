import { test, expect } from '@playwright/test';
import { resetMeals } from './_helpers';

test.describe('i18n', () => {
	test.beforeEach(async ({ request }) => {
		await resetMeals(request);
	});

	test('detects German from browser Accept-Language', async ({ browser }) => {
		const context = await browser.newContext({ locale: 'de-DE' });
		const page = await context.newPage();
		await page.goto('/');
		await expect(page.locator('h1')).toContainText('Mahlzeiten');
		await context.close();
	});

	test('detects non-German defaults to English', async ({ browser }) => {
		const context = await browser.newContext({ locale: 'fr-FR' });
		const page = await context.newPage();
		await page.goto('/');
		await expect(page.locator('h1')).toContainText('Meals');
		await context.close();
	});

	test('all UI strings translate to German', async ({ browser }) => {
		const context = await browser.newContext({ locale: 'de-DE' });
		const page = await context.newPage();
		await page.goto('/');

		// Header
		await expect(page.locator('h1')).toContainText('Mahlzeiten');

		// Search
		await expect(page.getByPlaceholder('Mahlzeiten suchen...')).toBeVisible();

		// Form heading
		await expect(page.getByRole('heading', { name: 'Mahlzeit hinzufügen' })).toBeVisible();

		// Form labels
		await expect(page.getByText('Name')).toBeVisible();
		await expect(page.getByText('Zutaten')).toBeVisible();

		// Submit button
		await expect(page.getByRole('button', { name: 'Hinzufügen', exact: true })).toBeVisible();

		// Empty state
		await expect(page.getByText('Deine Rezeptsammlung ist leer')).toBeVisible();
		await expect(page.getByText('Noch keine Mahlzeiten. Füge deine erste hinzu.')).toBeVisible();

		// All meals heading
		await expect(page.getByRole('heading', { name: 'Alle Mahlzeiten' })).toBeVisible();

		// Validation
		await page.getByRole('button', { name: 'Hinzufügen', exact: true }).click();
		await expect(page.getByText('Name ist erforderlich')).toBeVisible();

		await context.close();
	});
});
