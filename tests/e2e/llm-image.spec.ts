import { test, expect, type Page } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';
import { buildPng } from './_png';

const PHOTO = buildPng(8, 8);

test.describe('AI import image', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	async function openAiImport(page: Page) {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('dialog').getByRole('button', { name: 'AI import' }).click();
	}

	// Selects the custom provider with a dead endpoint: the model listing
	// fails fast and the model select is replaced by a text input.
	async function selectCustomModel(page: Page) {
		const dialog = page.getByRole('dialog');
		await dialog.getByRole('combobox').selectOption('custom');
		await dialog.getByLabel('Base URL').fill('http://127.0.0.1:1/v1/');
		await expect(dialog.getByPlaceholder('Model name (e.g. gpt-4o-mini)')).toBeVisible();
		await dialog.getByPlaceholder('Model name (e.g. gpt-4o-mini)').fill('test-model');
	}

	test('stages and removes a photo in the AI import tab', async ({ page }) => {
		await openAiImport(page);
		const dialog = page.getByRole('dialog');
		await dialog.locator('input[type="file"]').setInputFiles([
			{ name: 'front.png', mimeType: 'image/png', buffer: PHOTO },
			{ name: 'back.png', mimeType: 'image/png', buffer: PHOTO },
		]);
		const thumbs = dialog.locator('.multi-image-thumb');
		await expect(thumbs).toHaveCount(2);
		// No model selected yet → parse stays disabled even with photos.
		await expect(dialog.getByRole('button', { name: 'Parse with AI' })).toBeDisabled();
		// Remove the second photo; the first must remain in place.
		await thumbs.nth(1).getByRole('button', { name: 'Remove image' }).click();
		await expect(thumbs).toHaveCount(1);
		await expect(thumbs.nth(0)).toHaveAttribute('title', 'front.png');
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

	test('rejects a 6th photo with the limit message', async ({ page }) => {
		await openAiImport(page);
		const dialog = page.getByRole('dialog');
		const input = dialog.locator('input[type="file"]');
		await input.setInputFiles(
			Array.from({ length: 5 }, (_, i) => ({
				name: `p${i + 1}.png`, mimeType: 'image/png', buffer: PHOTO,
			})),
		);
		const thumbs = dialog.locator('.multi-image-thumb');
		await expect(thumbs).toHaveCount(5);
		// A 6th photo is rejected entirely; the 5 staged stay put.
		await input.setInputFiles({ name: 'p6.png', mimeType: 'image/png', buffer: PHOTO });
		await expect(dialog.getByText('At most 5 photos can be added').first()).toBeVisible();
		await expect(thumbs).toHaveCount(5);
	});

	test('sends all staged photos in order and clears them after parse', async ({ page }) => {
		let captured: { headers: Record<string, string>; body: Buffer } | null = null;
		await page.route('**/api/import/llm', async (route) => {
			captured = {
				headers: route.request().headers(),
				body: route.request().postDataBuffer() ?? Buffer.alloc(0),
			};
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					name: 'Merged Curry',
					ingredients: [{ name: 'chicken', quantity: '200 g' }],
					instructions: 'Cook.',
					imageBase64: null,
				}),
			});
		});

		await openAiImport(page);
		const dialog = page.getByRole('dialog');
		await selectCustomModel(page);
		await dialog.locator('.llm-hint-input').fill('front and back');
		const front = Buffer.from([0x01, 0x02, 0x03]);
		const back = Buffer.from([0x04, 0x05, 0x06]);
		await dialog.locator('input[type="file"]').setInputFiles([
			{ name: 'front.png', mimeType: 'image/png', buffer: front },
			{ name: 'back.png', mimeType: 'image/png', buffer: back },
		]);
		await expect(dialog.locator('.multi-image-thumb')).toHaveCount(2);

		await dialog.getByRole('button', { name: 'Parse with AI' }).click();

		// The intercepted request must carry the staged photos in order.
		await expect.poll(() => captured).toBeTruthy();
		const contentType = captured!.headers['content-type'] ?? '';
		const boundary = contentType.match(/boundary=(.+)$/)?.[1];
		expect(boundary).toBeTruthy();
		const delimiter = Buffer.from(`--${boundary}`);
		const imageParts: Buffer[] = [];
		let start = 0;
		for (;;) {
			const idx = captured!.body.indexOf(delimiter, start);
			if (idx === -1) break;
			const nextIdx = captured!.body.indexOf(delimiter, idx + delimiter.length);
			if (nextIdx === -1) break;
			const part = captured!.body.subarray(idx + delimiter.length, nextIdx);
			const headerEnd = part.indexOf(Buffer.from('\r\n\r\n'));
			if (headerEnd !== -1) {
				const header = part.subarray(0, headerEnd).toString();
				if (header.includes('name="image"')) {
					imageParts.push(part.subarray(headerEnd + 4, part.length - 2));
				}
			}
			start = nextIdx;
		}
		expect(imageParts).toHaveLength(2);
		expect(imageParts[0]).toEqual(front);
		expect(imageParts[1]).toEqual(back);

		// The draft pre-fills the manual form.
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Merged Curry');

		// Photos and hint are cleared after a successful parse.
		await page.getByRole('dialog').getByRole('button', { name: 'AI import' }).click();
		await expect(dialog.locator('.multi-image-thumb')).toHaveCount(0);
		await expect(dialog.locator('.llm-hint-input')).toHaveValue('');
	});

	test('parse stays disabled without hint or photos', async ({ page }) => {
		await openAiImport(page);
		const dialog = page.getByRole('dialog');
		await selectCustomModel(page);
		const parse = dialog.getByRole('button', { name: 'Parse with AI' });
		await expect(parse).toBeDisabled();
		// One photo is enough to enable parsing.
		await dialog.locator('input[type="file"]').setInputFiles({
			name: 'photo.png', mimeType: 'image/png', buffer: PHOTO,
		});
		await expect(parse).toBeEnabled();
		// Removing it (and having no hint) disables parsing again.
		await dialog.locator('.multi-image-thumb').getByRole('button', { name: 'Remove image' }).click();
		await expect(parse).toBeDisabled();
	});
});
