import { test, expect } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

// Minimal valid 1x1 transparent PNG (67 bytes).
const PNG_1x1_BASE64 =
	'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';

test.describe('Meal image via URL', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('given_valid_image_url_when_loaded_then_thumbnail_shown_and_meal_saved_with_image', async ({ page }) => {
		await page.route('**/api/import/image-url', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ imageBase64: PNG_1x1_BASE64 }),
			});
		});

		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();

		await page.getByLabel('Name', { exact: true }).fill('Photo Meal');
		await page.getByRole('textbox', { name: 'Ingredient name 1' }).fill('x');
		await page.getByLabel('Instructions').fill('Cook.');

		// Click the "From URL" tile to expand the URL input row
		await page.getByText('From URL').click();

		await page.getByPlaceholder('https://example.com/photo.jpg').fill('https://example.com/photo.jpg');
		await page.getByRole('button', { name: 'Load', exact: true }).click();

		await expect(page.locator('img.staged-image-preview')).toBeVisible();
		await expect(page.getByText('New image selected')).toBeVisible();

		await page.getByRole('dialog').getByRole('button', { name: /^(Add|Hinzufügen)$/ }).click();
		await expect(page.getByRole('dialog')).not.toBeVisible();
		await expect(page.getByRole('listitem').filter({ hasText: 'Photo Meal' })).toBeVisible();
	});

	test('given_image_url_unreachable_when_loaded_then_error_shown', async ({ page }) => {
		// The frontend maps an error message containing 'unreachable' to t('imageErrorUrlUnreachable').
		await page.route('**/api/import/image-url', async (route) => {
			await route.fulfill({
				status: 422,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'image URL unreachable' }),
			});
		});

		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();

		// Click the "From URL" tile to expand the URL input row
		await page.getByText('From URL').click();

		await page.getByPlaceholder('https://example.com/photo.jpg').fill('https://bad.example/404.jpg');
		await page.getByRole('button', { name: 'Load', exact: true }).click();

		// t('imageErrorUrlUnreachable') = 'Could not reach image URL'
		const error = page.locator('.form-error').filter({ hasText: 'Could not reach image URL' });
		await expect(error).toBeVisible();
	});
});
