import { test, expect } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';
import { buildPng } from './_png';

const PHOTO = buildPng(8, 8);

test.describe('AI import image', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	async function openAiImport(page: import('@playwright/test').Page) {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('dialog').getByRole('button', { name: 'AI import' }).click();
	}

	test('stages and removes a photo in the AI import tab', async ({ page }) => {
		await openAiImport(page);
		const dialog = page.getByRole('dialog');
		await dialog.locator('input[type="file"]').setInputFiles({
			name: 'photo.png', mimeType: 'image/png', buffer: PHOTO,
		});
		await expect(dialog.locator('.staged-image-preview')).toBeVisible();
		// No model selected yet → parse stays disabled even with a photo.
		await expect(dialog.getByRole('button', { name: 'Parse with AI' })).toBeDisabled();
		await dialog.getByRole('button', { name: 'Remove image' }).click();
		await expect(dialog.getByText('Image will be removed')).toBeVisible();
		await dialog.getByRole('button', { name: 'Cancel' }).click();
		await expect(dialog.locator('.staged-image-preview')).toHaveCount(0);
	});

	test('parses with a photo and no hint, reaching the backend', async ({ page }) => {
		await openAiImport(page);
		const dialog = page.getByRole('dialog');
		// The custom OpenAI-compatible provider is always selectable without
		// an API key; a dead endpoint makes the model listing fail fast.
		await dialog.getByRole('combobox').selectOption('custom');
		await dialog.getByLabel('Base URL').fill('http://127.0.0.1:1/v1/');
		// Model listing failure replaces the model select with a text input.
		await expect(dialog.getByPlaceholder('Model name (e.g. gpt-4o-mini)')).toBeVisible();
		await dialog.getByPlaceholder('Model name (e.g. gpt-4o-mini)').fill('test-model');
		await dialog.locator('input[type="file"]').setInputFiles({
			name: 'photo.png', mimeType: 'image/png', buffer: PHOTO,
		});
		const parse = dialog.getByRole('button', { name: 'Parse with AI' });
		await expect(parse).toBeEnabled();
		await parse.click();
		// Backend accepted model+image without a hint (no "at least one of
		// image or hint is required" 400); the request fails at the LLM
		// network call against the dead endpoint.
		await expect(dialog.getByText(/The AI request failed/)).toBeVisible();
	});
});
