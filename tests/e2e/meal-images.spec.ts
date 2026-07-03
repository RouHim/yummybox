import { test, expect } from '@playwright/test';
import { createMeal, resetMeals, setLocale } from './_helpers';
import { deflateSync } from 'node:zlib';

// ---------------------------------------------------------------------------
// PNG generator (produces Buffer objects for setInputFiles)
// ---------------------------------------------------------------------------

function buildPng(w: number, h: number): Buffer {
	// Build raw RGB pixel data
	const rawRowSize = 1 + w * 3;
	const raw = Buffer.alloc(rawRowSize * h);
	for (let y = 0; y < h; y++) {
		const off = y * rawRowSize;
		raw[off] = 0; // filter: None
		for (let x = 0; x < w; x++) {
			raw[off + 1 + x * 3] = 255;     // R
			raw[off + 1 + x * 3 + 1] = 0;   // G
			raw[off + 1 + x * 3 + 2] = 0;   // B
		}
	}
	const compressed = deflateSync(raw);

	// IHDR chunk
	const ihdrData = Buffer.alloc(13);
	ihdrData.writeUInt32BE(w, 0);
	ihdrData.writeUInt32BE(h, 4);
	ihdrData[8] = 8;  // bit depth
	ihdrData[9] = 2;  // color type RGB
	ihdrData[10] = 0; // compression
	ihdrData[11] = 0; // filter
	ihdrData[12] = 0; // interlace

	return Buffer.concat([
		Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), // PNG signature
		makePngChunk('IHDR', ihdrData),
		makePngChunk('IDAT', compressed),
		makePngChunk('IEND', Buffer.alloc(0)),
	]);
}

function makePngChunk(type: string, data: Buffer): Buffer {
	const lenB = Buffer.alloc(4);
	lenB.writeUInt32BE(data.length);
	const typeB = Buffer.from(type, 'ascii');
	const crcInput = Buffer.concat([typeB, data]);
	const crc = crc32(crcInput);
	const crcB = Buffer.alloc(4);
	crcB.writeUInt32BE(crc);
	return Buffer.concat([lenB, typeB, data, crcB]);
}

let _crcTable: Uint32Array | undefined;
function crcTable(): Uint32Array {
	if (_crcTable) return _crcTable;
	const t = new Uint32Array(256);
	for (let n = 0; n < 256; n++) {
		let c = n;
		for (let k = 0; k < 8; k++) {
			c = (c & 1) ? (0xEDB88320 ^ (c >>> 1)) : (c >>> 1);
		}
		t[n] = c;
	}
	_crcTable = t;
	return t;
}

function crc32(buf: Buffer): number {
	const table = crcTable();
	let c = 0xFFFFFFFF;
	for (let i = 0; i < buf.length; i++) {
		c = table[(c ^ buf[i]) & 0xFF]! ^ (c >>> 8);
	}
	return (c ^ 0xFFFFFFFF) >>> 0;
}

// Pre-build the images (sync, at module load)
const TINY_PNG = buildPng(1, 1);
const SMALL_PNG = buildPng(100, 100);
const OVERSIZED_PNG = buildPng(8000, 4500);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test.describe('Meal images', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('adds a meal with an image and shows a thumbnail', async ({ page }) => {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('dialog').getByLabel('Name', { exact: true }).fill('Pasta Photo');
		await page.getByRole('dialog').getByLabel('Instructions').fill('Cook.');
		await page.getByRole('dialog').getByRole('textbox', { name: 'Ingredient name 1' }).fill('noodles');
		await page.getByRole('dialog').locator('input[type="file"]').setInputFiles({
			name: 'photo.png',
			mimeType: 'image/png',
			buffer: SMALL_PNG,
		});
		await page.getByRole('dialog').getByRole('button', { name: 'Add', exact: true }).click();

		const mealCard = page.getByRole('listitem').filter({ hasText: 'Pasta Photo' });
		await expect(mealCard).toBeVisible();
		await expect(mealCard.locator('img.meal-card__hero')).toBeVisible();
	});

	test('editing a meal to add an image sets has_image', async ({ page, request }) => {
		await createMeal(page, 'No Image Yet', [{ name: 'stuff' }]);

		const mealCard = page.getByRole('listitem').filter({ hasText: 'No Image Yet' });
		await expect(mealCard.locator('img.meal-card__hero')).not.toBeAttached();

		await mealCard.hover();
		await mealCard.getByRole('button', { name: 'Edit' }).click();
		const editDialog = page.getByRole('dialog', { name: /^Edit: |^Bearbeiten: / });
		await editDialog.locator('input[type="file"]').setInputFiles({
			name: 'photo.png',
			mimeType: 'image/png',
			buffer: SMALL_PNG,
		});
		await editDialog.getByRole('button', { name: 'Save' }).click();

		await expect(page.getByRole('listitem').filter({ hasText: 'No Image Yet' }).locator('img.meal-card__hero')).toBeVisible();

		const res = await request.get('/api/meals?search=No Image Yet');
		const meals = await res.json() as Array<{ id: number; has_image: boolean }>;
		expect(meals[0]?.has_image).toBe(true);
	});



	test('uploading a new image replaces the old one', async ({ page, request }) => {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('dialog').getByLabel('Name', { exact: true }).fill('Replace Test');
		await page.getByRole('dialog').getByLabel('Instructions').fill('Cook.');
		await page.getByRole('dialog').getByRole('textbox', { name: 'Ingredient name 1' }).fill('test');
		await page.getByRole('dialog').locator('input[type="file"]').setInputFiles({
			name: 'photo.png',
			mimeType: 'image/png',
			buffer: SMALL_PNG,
		});
		await page.getByRole('dialog').getByRole('button', { name: 'Add', exact: true }).click();

		const mealCard = page.getByRole('listitem').filter({ hasText: 'Replace Test' });
		await expect(mealCard).toBeVisible({ timeout: 10_000 });

		const listRes = await request.get('/api/meals?search=Replace Test');
		const list = await listRes.json() as Array<{ id: number }>;
		expect(list.length).toBeGreaterThanOrEqual(1);
		const mealId = list[0]!.id;
		const imgRes = await request.get(`/api/meals/${mealId}/image`);
		expect(imgRes.status()).toBe(200);
		const beforeBytes = Buffer.from(await imgRes.body());

		await mealCard.hover();
		await mealCard.getByRole('button', { name: 'Edit' }).click();
		const editDialog = page.getByRole('dialog', { name: /^Edit: |^Bearbeiten: / });
		await editDialog.locator('input[type="file"]').setInputFiles({
			name: 'tiny.png',
			mimeType: 'image/png',
			buffer: TINY_PNG,
		});
		await editDialog.getByRole('button', { name: 'Save' }).click();

		await expect(mealCard.locator('img.meal-card__hero')).toBeVisible();
		const afterRes = await request.get(`/api/meals/${mealId}/image`);
		const afterBytes = Buffer.from(await afterRes.body());
		expect(Buffer.compare(beforeBytes, afterBytes)).not.toBe(0);
	});
	test('removing an image via the form clears has_image', async ({ page, request }) => {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('dialog').getByLabel('Name', { exact: true }).fill('Remove Test');
		await page.getByRole('dialog').getByLabel('Instructions').fill('Cook.');
		await page.getByRole('dialog').getByRole('textbox', { name: 'Ingredient name 1' }).fill('test');
		await page.getByRole('dialog').locator('input[type="file"]').setInputFiles({
			name: 'photo.png',
			mimeType: 'image/png',
			buffer: SMALL_PNG,
		});
		await page.getByRole('dialog').getByRole('button', { name: 'Add', exact: true }).click();

		const mealCard = page.getByRole('listitem').filter({ hasText: 'Remove Test' });
		await expect(mealCard.locator('img.meal-card__hero')).toBeVisible();

		await mealCard.hover();
		await mealCard.getByRole('button', { name: 'Edit' }).click();
		const editDialog = page.getByRole('dialog', { name: /^Edit: |^Bearbeiten: / });
		await editDialog.getByRole('button', { name: 'Remove image' }).click({ force: true });
		await editDialog.getByRole('button', { name: 'Save' }).click();

		const updatedCard = page.getByRole('listitem').filter({ hasText: 'Remove Test' });
		await expect(updatedCard).toBeVisible();
		await expect(updatedCard.locator('img.meal-card__hero')).not.toBeAttached();

		const res = await request.get('/api/meals?search=Remove Test');
		const meals = await res.json() as Array<{ has_image: boolean }>;
		expect(meals[0]?.has_image).toBe(false);
	});

	test('uploading a non-image file shows server error inline', async ({ page }) => {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('dialog').getByLabel('Name', { exact: true }).fill('Bad File');
		await page.getByRole('dialog').getByLabel('Instructions').fill('Cook.');
		await page.getByRole('dialog').getByRole('textbox', { name: 'Ingredient name 1' }).fill('test');
		await page.getByRole('dialog').locator('input[type="file"]').setInputFiles({
			name: 'readme.txt',
			mimeType: 'text/plain',
			buffer: Buffer.from('this is not an image'),
		});
		await page.getByRole('dialog').getByRole('button', { name: 'Add', exact: true }).click();
		// The exact error message may vary; check form-error appears
		await expect(page.getByRole('dialog').locator('.form-error').first()).toBeVisible();
		await expect(page.getByRole('listitem').filter({ hasText: 'Bad File' })).not.toBeAttached();
	});

	test('meals without images render no img tag', async ({ page }) => {
		await createMeal(page, 'Plain Meal', [{ name: 'x' }]);

		const mealCard = page.getByRole('listitem').filter({ hasText: 'Plain Meal' });
		await expect(mealCard).toBeVisible();
		await expect(mealCard.locator('img')).not.toBeAttached();
	});

	test('uploading an image larger than 4K downscales on the longer edge', async ({ page, request }) => {
		test.setTimeout(60_000);

		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('dialog').getByLabel('Name', { exact: true }).fill('Big Photo');
		await page.getByRole('dialog').getByLabel('Instructions').fill('Cook.');
		await page.getByRole('dialog').getByRole('textbox', { name: 'Ingredient name 1' }).fill('test');
		await page.getByRole('dialog').locator('input[type="file"]').setInputFiles({
			name: 'oversized.png',
			mimeType: 'image/png',
			buffer: OVERSIZED_PNG,
		});
		await page.getByRole('dialog').getByRole('button', { name: 'Add', exact: true }).click();

		await expect(page.getByRole('listitem').filter({ hasText: 'Big Photo' })).toBeVisible({ timeout: 30_000 });

		const mealsRes = await request.get('/api/meals?search=Big Photo');
		const meals = await mealsRes.json() as Array<{ id: number }>;
		const mealId = meals[0]!.id;

		const imgRes = await request.get(`/api/meals/${mealId}/image`);
		expect(imgRes.status()).toBe(200);
		expect(imgRes.headers()['content-type']).toBe('image/jpeg');

		// Decode JPEG dimensions from SOF marker
		const jpegBytes = Buffer.from(await imgRes.body());
		let w = 0, h = 0;
		for (let i = 0; i < jpegBytes.length - 9; i++) {
			if (
				jpegBytes[i] === 0xFF &&
				(jpegBytes[i + 1] === 0xC0 ||
					jpegBytes[i + 1] === 0xC1 ||
					jpegBytes[i + 1] === 0xC2)
			) {
				h = jpegBytes.readUInt16BE(i + 5);
				w = jpegBytes.readUInt16BE(i + 7);
				break;
			}
		}
		expect(w).toBe(3840);
		expect(h).toBe(2160);
	});
});
