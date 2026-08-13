import { describe, it, expect } from 'vitest';
import {
	MAX_GENERATE_IMAGE_BYTES,
	MAX_GENERATE_TOTAL_BYTES,
	validateGenerateImage,
	validateGenerateImageTotal,
} from './multi-image';

describe('validateGenerateImage', () => {
	it('accepts a small PNG', () => {
		const file = new File([new Uint8Array(8)], 'a.png', { type: 'image/png' });
		expect(validateGenerateImage(file)).toBeNull();
	});

	it('rejects non-image files', () => {
		const file = new File(['x'], 'a.txt', { type: 'text/plain' });
		expect(validateGenerateImage(file)).toBe('generateErrorUnsupportedImageType');
	});

	it('rejects image types outside the backend allowlist', () => {
		for (const type of ['image/heic', 'image/avif', 'image/bmp', 'image/tiff']) {
			const file = new File(['x'], 'a.bin', { type });
			expect(validateGenerateImage(file)).toBe('generateErrorUnsupportedImageType');
		}
	});

	it('accepts every backend allowlisted type', () => {
		for (const type of ['image/jpeg', 'image/png', 'image/webp', 'image/gif']) {
			const file = new File(['x'], 'a.img', { type });
			expect(validateGenerateImage(file)).toBeNull();
		}
	});

	it('rejects files over 20 MB', () => {
		const big = new File([new Uint8Array(MAX_GENERATE_IMAGE_BYTES + 1)], 'big.jpg', { type: 'image/jpeg' });
		expect(validateGenerateImage(big)).toBe('generateErrorImageTooLarge');
	});
});

describe('validateGenerateImageTotal', () => {
	it('accepts a set under the total cap', () => {
		const files = [new File([new Uint8Array(8)], 'a.png', { type: 'image/png' })];
		expect(validateGenerateImageTotal(files)).toBeNull();
	});

	it('rejects sets whose combined size exceeds 50 MB even when each file is under the per-file cap', () => {
		const files = [
			new File([new Uint8Array(18 * 1024 * 1024)], 'a.jpg', { type: 'image/jpeg' }),
			new File([new Uint8Array(18 * 1024 * 1024)], 'b.jpg', { type: 'image/jpeg' }),
			new File([new Uint8Array(18 * 1024 * 1024)], 'c.jpg', { type: 'image/jpeg' }),
		];
		expect(validateGenerateImageTotal(files)).toBe('imageTotalTooLarge');
	});

	it('rejects a set at the 50 MB body limit (framing overhead included)', () => {
		const files = [new File([new Uint8Array(50 * 1024 * 1024)], 'a.jpg', { type: 'image/jpeg' })];
		expect(validateGenerateImageTotal(files)).toBe('imageTotalTooLarge');
	});

	it('allows a set exactly at the client cap', () => {
		const files = [new File([new Uint8Array(MAX_GENERATE_TOTAL_BYTES)], 'a.jpg', { type: 'image/jpeg' })];
		expect(validateGenerateImageTotal(files)).toBeNull();
	});
});
