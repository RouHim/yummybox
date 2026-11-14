import { test, expect, type Page } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

/**
 * UI wiring of the ZIP / Mealie import tabs. The archive itself is never
 * parsed here: the import endpoint is intercepted and answers with the shape
 * `src/export_import.rs::ZipImportResult` actually serializes
 * (`{ created: Meal[], skipped: number, failed: [{ source, reason }] }`).
 */

const CRC_TABLE = (() => {
	const table = new Uint32Array(256);
	for (let i = 0; i < 256; i++) {
		let c = i;
		for (let bit = 0; bit < 8; bit++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
		table[i] = c >>> 0;
	}
	return table;
})();

function crc32(bytes: Uint8Array): number {
	let crc = 0xffffffff;
	for (const byte of bytes) crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
	return (crc ^ 0xffffffff) >>> 0;
}

/** Minimal valid STORE-mode ZIP archive holding one entry. */
function storedZip(entryName: string, content: string): Buffer {
	const name = new TextEncoder().encode(entryName);
	const data = new TextEncoder().encode(content);
	const crc = crc32(data);

	const local = Buffer.alloc(30 + name.length + data.length);
	local.writeUInt32LE(0x04034b50, 0); // local file header signature
	local.writeUInt16LE(20, 4); // version needed to extract
	local.writeUInt16LE(0, 6); // flags
	local.writeUInt16LE(0, 8); // compression method: stored
	local.writeUInt16LE(0, 10); // modification time
	local.writeUInt16LE(0x21, 12); // modification date (1980-01-01)
	local.writeUInt32LE(crc, 14);
	local.writeUInt32LE(data.length, 18); // compressed size
	local.writeUInt32LE(data.length, 22); // uncompressed size
	local.writeUInt16LE(name.length, 26);
	local.writeUInt16LE(0, 28); // extra field length
	local.set(name, 30);
	local.set(data, 30 + name.length);

	const central = Buffer.alloc(46 + name.length);
	central.writeUInt32LE(0x02014b50, 0); // central directory header signature
	central.writeUInt16LE(20, 4); // version made by
	central.writeUInt16LE(20, 6); // version needed to extract
	central.writeUInt16LE(0, 8); // flags
	central.writeUInt16LE(0, 10); // compression method: stored
	central.writeUInt16LE(0, 12); // modification time
	central.writeUInt16LE(0x21, 14); // modification date
	central.writeUInt32LE(crc, 16);
	central.writeUInt32LE(data.length, 20);
	central.writeUInt32LE(data.length, 24);
	central.writeUInt16LE(name.length, 28);
	central.writeUInt16LE(0, 30); // extra field length
	central.writeUInt16LE(0, 32); // comment length
	central.writeUInt16LE(0, 34); // disk number start
	central.writeUInt16LE(0, 36); // internal attributes
	central.writeUInt32LE(0, 38); // external attributes
	central.writeUInt32LE(0, 42); // relative offset of local header
	central.set(name, 46);

	const end = Buffer.alloc(22);
	end.writeUInt32LE(0x06054b50, 0); // end of central directory signature
	end.writeUInt16LE(0, 4); // this disk
	end.writeUInt16LE(0, 6); // disk with central directory
	end.writeUInt16LE(1, 8); // entries on this disk
	end.writeUInt16LE(1, 10); // entries total
	end.writeUInt32LE(central.length, 12);
	end.writeUInt32LE(local.length, 16);
	end.writeUInt16LE(0, 20); // comment length

	return Buffer.concat([local, central, end]);
}

const ZIP_FILE = {
	name: 'yummybox-export.zip',
	mimeType: 'application/zip',
	buffer: storedZip('recipes.json', JSON.stringify({ '@context': {}, '@graph': [] })),
};

async function openZipTab(page: Page) {
	await page.goto('/meals');
	await page.getByRole('button', { name: 'More' }).click();
	await page.getByRole('menuitem', { name: 'Import' }).click();
	const dialog = page.getByRole('dialog');
	await expect(dialog).toBeVisible();
	await dialog.getByRole('button', { name: 'ZIP file' }).click();
	await expect(dialog.getByText('Choose a YummyBox ZIP file')).toBeVisible();
	return dialog;
}

test.describe('ZIP import', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('given_zip_import_with_no_new_meals_when_submitted_then_summary_and_failures_shown', async ({ page }) => {
		let posted: string | null = null;
		await page.route('**/api/import/zip', async (route) => {
			const request = route.request();
			posted = request.headers()['content-type'] ?? '';
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					created: [],
					skipped: 1,
					failed: [{ source: 'recipes.json', reason: 'duplicate' }],
				}),
			});
		});

		const dialog = await openZipTab(page);
		await dialog.locator('input[type="file"]').first().setInputFiles(ZIP_FILE);
		await dialog.getByRole('button', { name: 'Import from ZIP' }).click();

		// Nothing was created, so the app keeps the dialog open and reports.
		await expect(dialog.getByText('0 meals imported, 1 skipped')).toBeVisible();
		await expect(dialog.getByText('recipes.json')).toBeVisible();
		await expect(dialog.getByText('import error')).toBeVisible();
		await expect(dialog).toBeVisible();
		expect(posted).toContain('multipart/form-data');
		await expect(page).toHaveURL(/\/meals$/);
	});

	test('given_zip_import_with_new_meals_when_submitted_then_dialog_closes_and_list_reloads', async ({ page }) => {
		await page.route('**/api/import/zip', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					created: [
						{
							id: 4242,
							name: 'Imported stew',
							ingredients: [],
							instructions: 'Simmer',
							portions: null,
							source_url: null,
							last_planned_at: null,
							created_at: '2026-01-01T00:00:00Z',
							updated_at: '2026-01-01T00:00:00Z',
							has_image: false,
						},
					],
					skipped: 1,
					failed: [],
				}),
			});
		});

		const dialog = await openZipTab(page);
		await dialog.locator('input[type="file"]').first().setInputFiles(ZIP_FILE);
		await dialog.getByRole('button', { name: 'Import from ZIP' }).click();

		await expect(dialog).not.toBeVisible();
		await expect(page).toHaveURL(/\/meals$/);
	});

	test('given_zip_import_rejected_when_submitted_then_error_message_shown', async ({ page }) => {
		await page.route('**/api/import/zip', async (route) => {
			await route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'not a valid archive' }),
			});
		});

		const dialog = await openZipTab(page);
		await dialog.locator('input[type="file"]').first().setInputFiles(ZIP_FILE);
		await dialog.getByRole('button', { name: 'Import from ZIP' }).click();

		await expect(dialog.getByRole('alert')).toContainText('not a valid archive');
		await expect(dialog).toBeVisible();
	});

	test('given_mealie_tab_when_opened_then_mealie_import_posts_to_mealie_endpoint', async ({ page }) => {
		let postedToMealie: string | null = null;
		await page.route('**/api/import/mealie', async (route) => {
			postedToMealie = route.request().url();
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ created: [], skipped: 2, failed: [] }),
			});
		});

		const dialog = await openZipTab(page);

		// The ZIP tab hosts both pickers. Neither input rejects a wrong file
		// type in script: the only client-side gate is the `accept` hint, which
		// the browser applies in its own file chooser.
		const zipInput = dialog.getByLabel('Choose a YummyBox ZIP file');
		const mealieInput = dialog.getByLabel('Choose a Mealie ZIP file');
		await expect(zipInput).toHaveAttribute('accept', '.zip');
		await expect(mealieInput).toHaveAttribute('accept', '.zip');

		const mealieButton = dialog.getByRole('button', { name: 'Import from Mealie' });
		await expect(mealieButton).toBeDisabled();
		await mealieInput.setInputFiles(ZIP_FILE);
		await expect(mealieButton).toBeEnabled();
		await mealieButton.click();

		await expect(dialog.getByText('0 meals imported, 2 skipped')).toBeVisible();
		expect(postedToMealie).toContain('/api/import/mealie');
	});
});
