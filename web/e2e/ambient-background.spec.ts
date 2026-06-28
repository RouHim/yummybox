import { test, expect } from '@playwright/test';

test.describe('Ambient background', () => {
	test('app-ambient element exists and covers viewport', async ({ page }) => {
		await page.goto('/');
		await page.waitForSelector('.app-ambient', { state: 'attached', timeout: 10000 });

		const el = page.locator('.app-ambient');
		await expect(el).toBeVisible();

		const box = await el.boundingBox();
		expect(box).not.toBeNull();
		expect(box!.width).toBeGreaterThan(100);
		expect(box!.height).toBeGreaterThan(100);
	});

	test('background-image CSS is set and resolves to a real image', async ({ page }) => {
		await page.goto('/');
		await page.waitForSelector('.app-ambient', { state: 'attached', timeout: 10000 });

		const bgImage = await page.locator('.app-ambient').evaluate(el =>
			window.getComputedStyle(el).backgroundImage
		);
		expect(bgImage).not.toBe('none');
		expect(bgImage).toMatch(/url\(".+\.(jpg|jpeg|svg|png|webp)"\)/);

		// Extract the URL and verify the image loads
		const urlMatch = bgImage.match(/url\("(.+)"\)/);
		expect(urlMatch).not.toBeNull();
		const imageUrl = urlMatch![1];

		const response = await page.request.get(imageUrl);
		expect(response.status()).toBe(200);
		expect(response.headers()['content-type']).toMatch(/image\/(jpeg|svg\+xml|png|webp)/);

		const body = await response.body();
		expect(body.length).toBeGreaterThan(1000); // not a tiny/empty image
	});

	test('background image has visible pixel variance — not a solid color', async ({ page }) => {
		await page.goto('/');
		await page.waitForSelector('.app-ambient', { state: 'attached', timeout: 10000 });

		// Get the image URL from CSS
		const imageUrl: string = await page.locator('.app-ambient').evaluate(el => {
			const bg = window.getComputedStyle(el).backgroundImage;
			const m = bg.match(/url\("(.+)"\)/);
			return m ? m[1] : '';
		});
		expect(imageUrl).toBeTruthy();

		// Fetch the image and check pixel variance via canvas
		const hasVariance = await page.evaluate(async (url) => {
			const img = document.createElement('img');
			img.crossOrigin = 'anonymous';
			const { promise, resolve, reject } = Promise.withResolvers<void>();
			img.onload = () => resolve();
			img.onerror = () => reject(new Error('Image failed to load'));
			img.src = url;
			await promise;

			// Draw to canvas and sample pixels
			const canvas = document.createElement('canvas');
			canvas.width = 100;
			canvas.height = 100;
			const ctx = canvas.getContext('2d')!;
			ctx.drawImage(img, 0, 0, 100, 100);
			const imageData = ctx.getImageData(0, 0, 100, 100);
			const pixels = imageData.data;

			// Check if all pixels are identical (solid color)
			const firstR = pixels[0];
			const firstG = pixels[1];
			const firstB = pixels[2];
			let sameCount = 0;
			let total = 0;
			for (let i = 0; i < pixels.length; i += 4) {
				total++;
				if (pixels[i] === firstR && pixels[i + 1] === firstG && pixels[i + 2] === firstB) {
					sameCount++;
				}
			}

			// Also compute std dev as a more robust measure
			let sumR = 0, sumG = 0, sumB = 0;
			for (let i = 0; i < pixels.length; i += 4) {
				sumR += pixels[i];
				sumG += pixels[i + 1];
				sumB += pixels[i + 2];
			}
			const meanR = sumR / total;
			const meanG = sumG / total;
			const meanB = sumB / total;
			let varR = 0, varG = 0, varB = 0;
			for (let i = 0; i < pixels.length; i += 4) {
				varR += (pixels[i] - meanR) ** 2;
				varG += (pixels[i + 1] - meanG) ** 2;
				varB += (pixels[i + 2] - meanB) ** 2;
			}
			const stdDev = Math.sqrt((varR + varG + varB) / (total * 3));

			return {
				sameRatio: sameCount / total,
				stdDev,
				meanR: Math.round(meanR),
				meanG: Math.round(meanG),
				meanB: Math.round(meanB),
				firstPixel: [firstR, firstG, firstB],
			};
		}, imageUrl);

		// The image should NOT be a solid color: < 95% same pixels
		expect(hasVariance.sameRatio).toBeLessThan(0.95);
		// Standard deviation should be meaningful: > 2 (out of 255)
		expect(hasVariance.stdDev).toBeGreaterThan(2);

		console.log(`Image stats: mean=rgb(${hasVariance.meanR},${hasVariance.meanG},${hasVariance.meanB}), stdDev=${hasVariance.stdDev.toFixed(1)}, solidRatio=${(hasVariance.sameRatio * 100).toFixed(1)}%`);
	});

	test('footer has no Pexels attribution after palette redesign', async ({ page }) => {
		await page.goto('/');
		await page.waitForSelector('.site-footer', { state: 'attached', timeout: 10000 });

		const footer = page.locator('.site-footer');
		await expect(footer).toBeVisible();

		// Attribution was removed in Forest palette redesign — original images replaced
		const link = footer.locator('a');
		await expect(link).toHaveCount(0);
	});
});

test.describe('Ambient background — dark mode', () => {
	test.use({ colorScheme: 'dark' });

	test('dark background image has visible pixel variance', async ({ page }) => {
		await page.goto('/');
		await page.waitForSelector('.app-ambient', { state: 'attached', timeout: 10000 });

		const imageUrl: string = await page.locator('.app-ambient').evaluate(el => {
			const bg = window.getComputedStyle(el).backgroundImage;
			const m = bg.match(/url\("(.+)"\)/);
			return m ? m[1] : '';
		});
		expect(imageUrl).toContain('ambient-dark');

		const hasVariance = await page.evaluate(async (url) => {
			const img = document.createElement('img');
			img.crossOrigin = 'anonymous';
			const { promise, resolve, reject } = Promise.withResolvers<void>();
			img.onload = () => resolve();
			img.onerror = () => reject(new Error('Image failed to load'));
			img.src = url;
			await promise;

			const canvas = document.createElement('canvas');
			canvas.width = 100;
			canvas.height = 100;
			const ctx = canvas.getContext('2d')!;
			ctx.drawImage(img, 0, 0, 100, 100);
			const imageData = ctx.getImageData(0, 0, 100, 100);
			const pixels = imageData.data;

			const total = pixels.length / 4;
			let sumR = 0, sumG = 0, sumB = 0;
			for (let i = 0; i < pixels.length; i += 4) {
				sumR += pixels[i];
				sumG += pixels[i + 1];
				sumB += pixels[i + 2];
			}
			const meanR = sumR / total;
			const meanG = sumG / total;
			const meanB = sumB / total;
			let varR = 0, varG = 0, varB = 0;
			for (let i = 0; i < pixels.length; i += 4) {
				varR += (pixels[i] - meanR) ** 2;
				varG += (pixels[i + 1] - meanG) ** 2;
				varB += (pixels[i + 2] - meanB) ** 2;
			}
			const stdDev = Math.sqrt((varR + varG + varB) / (total * 3));

			const firstR = pixels[0], firstG = pixels[1], firstB = pixels[2];
			let sameCount = 0;
			for (let i = 0; i < pixels.length; i += 4) {
				if (pixels[i] === firstR && pixels[i+1] === firstG && pixels[i+2] === firstB) sameCount++;
			}

			return { meanR: Math.round(meanR), meanG: Math.round(meanG), meanB: Math.round(meanB), stdDev, solidRatio: sameCount / total };
		}, imageUrl);

		expect(hasVariance.solidRatio).toBeLessThan(0.95);
		expect(hasVariance.stdDev).toBeGreaterThan(2);
		console.log(`Dark image stats: mean=rgb(${hasVariance.meanR},${hasVariance.meanG},${hasVariance.meanB}), stdDev=${hasVariance.stdDev.toFixed(1)}, solidRatio=${(hasVariance.solidRatio*100).toFixed(1)}%`);
	});

	test('dark mode background color is dark, not light', async ({ page }) => {
		await page.goto('/');
		await page.waitForSelector('.app-ambient', { state: 'attached', timeout: 10000 });

		const bgColor = await page.locator('.app-ambient').evaluate(el =>
			window.getComputedStyle(el).backgroundColor
		);
		// Parse RGB
		const rgb = bgColor.match(/rgb\((\d+),\s*(\d+),\s*(\d+)\)/);
		expect(rgb).not.toBeNull();
		const r = parseInt(rgb![1]);
		const g = parseInt(rgb![2]);
		const b = parseInt(rgb![3]);

		// Dark mode bg should be dark: each channel < 80
		expect(r).toBeLessThan(80);
		expect(g).toBeLessThan(80);
		expect(b).toBeLessThan(80);
	});
});
