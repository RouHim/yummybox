import { describe, it, expect } from 'vitest';
import { MAX_GENERATE_IMAGE_BYTES, validateGenerateImage } from './multi-image';

describe('validateGenerateImage', () => {
	it('accepts a small PNG', () => {
		const file = new File([new Uint8Array(8)], 'a.png', { type: 'image/png' });
		expect(validateGenerateImage(file)).toBeNull();
	});

	it('rejects non-image files', () => {
		const file = new File(['x'], 'a.txt', { type: 'text/plain' });
		expect(validateGenerateImage(file)).toBe('generateErrorNotImage');
	});

	it('rejects files over 20 MB', () => {
		const big = new File([new Uint8Array(MAX_GENERATE_IMAGE_BYTES + 1)], 'big.jpg', { type: 'image/jpeg' });
		expect(validateGenerateImage(big)).toBe('generateErrorImageTooLarge');
	});
});
