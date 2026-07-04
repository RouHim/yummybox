import { test, expect } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

test.describe('language switcher', () => {
	test.beforeEach(async ({ request }) => {
		await resetMeals(request);
	});

	test('given_english_locale_when_german_selected_then_all_strings_translate', async ({ page }) => {
		await page.goto('/meals');

		await page.getByRole('button', { name: /^Language$|^Sprache$/ }).click();
		await expect(page.getByRole('listbox')).toBeVisible();
		await page.getByRole('option', { name: 'Deutsch' }).click();

		await expect(page.locator('html')).toHaveAttribute('lang', 'de');
		await expect(page.getByRole('link', { name: 'Mahlzeiten' })).toBeVisible();
		expect(await page.evaluate(() => localStorage.getItem('yummybox-locale'))).toBe('de');
	});

	test('given_german_locale_when_english_selected_then_strings_revert', async ({ page }) => {
		await setLocale(page, 'de');
		await page.goto('/meals');

		await page.getByRole('button', { name: /^Language$|^Sprache$/ }).click();
		await expect(page.getByRole('listbox')).toBeVisible();
		await page.getByRole('option', { name: 'English' }).click();

		await expect(page.locator('html')).toHaveAttribute('lang', 'en');
		await expect(page.getByRole('link', { name: 'Meals' })).toBeVisible();
		expect(await page.evaluate(() => localStorage.getItem('yummybox-locale'))).toBe('en');
	});

	test('given_any_locale_when_system_selected_then_navigator_drives_language', async ({ page }) => {
		await setLocale(page, 'en');
		await page.goto('/meals');

		await page.getByRole('button', { name: /^Language$|^Sprache$/ }).click();
		await expect(page.getByRole('listbox')).toBeVisible();
		await page.getByRole('option', { name: 'System' }).click();

		expect(await page.evaluate(() => localStorage.getItem('yummybox-locale'))).toBe('system');

		const navigatorLanguage = await page.evaluate(() => navigator.language);
		const htmlLang = await page.locator('html').getAttribute('lang');
		expect(htmlLang).toBeTruthy();
		expect(navigatorLanguage.startsWith(htmlLang as string)).toBe(true);
		expect((htmlLang as string).startsWith('en')).toBe(true);
	});
});
