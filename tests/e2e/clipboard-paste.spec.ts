import { test, expect } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

test.describe('Clipboard image paste', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('given_image_file_pasted_when_paste_fires_then_thumbnail_shown', async ({ page }) => {
		await page.goto('/meals');
		// Open add-meal dialog
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();

		// Some headless/browser contexts do not allow constructing synthetic ClipboardEvents,
		// or the resulting event has no files. Detect and skip rather than fail.
		const supportsSyntheticPaste = await page.evaluate(() => {
			try {
				const dt = new DataTransfer();
				dt.items.add(new File([], 'probe.png', { type: 'image/png' }));
				const ev = new ClipboardEvent('paste', { clipboardData: dt, bubbles: true });
				return Boolean(ev.clipboardData && ev.clipboardData.files.length > 0);
			} catch {
				return false;
			}
		});
		test.skip(!supportsSyntheticPaste, 'ClipboardEvent not supported in headless Chromium');

		// Focus the image field container that handles paste
		const imageField = page.locator('.field[tabindex="0"]').first();
		await imageField.focus();

		// Dispatch a synthetic paste event carrying a PNG file
		await page.evaluate(() => {
			const dt = new DataTransfer();
			dt.items.add(
				new File(
					[new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])],
					'pasted.png',
					{ type: 'image/png' }
				)
			);
			const el = document.querySelector('.field[tabindex="0"]');
			el?.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true }));
		});

		// Staged preview should render
		await expect(page.locator('img.staged-image-preview')).toBeVisible();
		await expect(page.getByText('New image selected')).toBeVisible();
	});
});
