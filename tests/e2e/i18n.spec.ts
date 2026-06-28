import { test, expect } from '@playwright/test';
import { resetMeals } from './_helpers';

test.describe('i18n', () => {
	test.beforeEach(async ({ request }) => {
		await resetMeals(request);
	});

	test('detects German from browser Accept-Language', async ({ browser }) => {
		const context = await browser.newContext({ locale: 'de-DE' });
		const page = await context.newPage();
		await page.goto('/meals');
		await expect(page.getByRole('link', { name: 'Mahlzeiten' })).toBeVisible();
		await context.close();
	});

	test('detects non-German defaults to English', async ({ browser }) => {
		const context = await browser.newContext({ locale: 'fr-FR' });
		const page = await context.newPage();
		await page.goto('/meals');
		await expect(page.getByRole('link', { name: 'Meals' })).toBeVisible();
		await context.close();
	});

	test('all UI strings translate to German', async ({ browser }) => {
		const context = await browser.newContext({ locale: 'de-DE' });
		const page = await context.newPage();
		await page.goto('/meals');

		// Header
		await expect(page.getByRole('link', { name: 'Mahlzeiten' })).toBeVisible();

		// Search
		await expect(page.getByPlaceholder('Mahlzeiten suchen...')).toBeVisible();

		// Open the add-meal modal
		await page.getByRole('button', { name: 'Mahlzeit hinzufügen' }).click();
		const dialog = page.getByRole('dialog', { name: 'Mahlzeit hinzufügen' });
		await expect(dialog).toBeVisible();

		// Form heading
		await expect(dialog.getByRole('heading', { name: 'Mahlzeit hinzufügen' })).toBeVisible();

		// Form labels
		await expect(dialog.getByLabel('Name', { exact: true })).toBeVisible();
		await expect(dialog.getByText('Zutaten')).toBeVisible();

		// Submit button
		await expect(dialog.getByRole('button', { name: 'Hinzufügen', exact: true })).toBeVisible();

		// Empty state
		await expect(page.getByText('Deine Rezeptsammlung ist leer')).toBeVisible();
		await expect(page.getByText('Noch keine Mahlzeiten. Füge deine erste hinzu.')).toBeVisible();

		// Validation
		await dialog.getByRole('button', { name: 'Hinzufügen', exact: true }).click();
		await expect(dialog.getByText('Name ist erforderlich')).toBeVisible();

		await context.close();
	});
});
