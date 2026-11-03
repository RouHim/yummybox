import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('./i18n', () => ({ t: (key: string) => key }));
vi.mock('./api', () => {
	class ApiError extends Error {
		code: string | null;
		status: number;
		constructor(message: string, code: string | null = null, status = 0) {
			super(message);
			this.name = 'ApiError';
			this.code = code;
			this.status = status;
		}
	}
	return { ApiError, loadImageFromUrl: vi.fn() };
});

import { ApiError } from './api';
import { ImageCapture, imageUrlErrorMessage, looksLikeImage } from './image-stage.svelte';

function file(name: string, type = 'image/png', size = 1024): File {
	return new File([new Uint8Array(size)], name, { type });
}

function capture(options: { maxFiles?: number; totalBytes?: number } = {}) {
	const onchange = vi.fn();
	const onerror = vi.fn();
	const stage = new ImageCapture(
		{
			maxFiles: options.maxFiles ?? 5,
			validateFile: (candidate) => (looksLikeImage(candidate) ? null : 'imageErrorNotImage'),
			validateTotal: (files) =>
				files.reduce((sum, candidate) => sum + candidate.size, 0) > (options.totalBytes ?? Infinity)
					? 'imageTotalTooLarge'
					: null,
		},
		{ onchange, onerror }
	);
	return { stage, onchange, onerror };
}

beforeEach(() => {
	vi.clearAllMocks();
});

describe('looksLikeImage', () => {
	it('accepts an image MIME type even without an extension', () => {
		expect(looksLikeImage(file('photo', 'image/webp'))).toBe(true);
	});

	it('accepts a known extension when the browser omits the MIME type', () => {
		expect(looksLikeImage(file('scan.TIFF', ''))).toBe(true);
	});

	it('rejects a file that is neither an image type nor a known extension', () => {
		expect(looksLikeImage(file('notes.txt', 'text/plain'))).toBe(false);
	});
});

describe('ImageCapture staging', () => {
	it('stages accepted files and notifies once', () => {
		const { stage, onchange, onerror } = capture();

		stage.add([file('a.png'), file('b.webp')]);

		expect(stage.files.map((f) => f.name)).toEqual(['a.png', 'b.webp']);
		expect(onchange).toHaveBeenCalledWith(stage.files);
		expect(onerror).toHaveBeenLastCalledWith(null);
	});

	it('rejects the whole add when one file is not an image', () => {
		const { stage, onchange, onerror } = capture();

		stage.add([file('a.png'), file('notes.txt', 'text/plain')]);

		expect(stage.files).toEqual([]);
		expect(stage.error).toBe('imageErrorNotImage');
		expect(onchange).not.toHaveBeenCalled();
		expect(onerror).toHaveBeenCalledWith('imageErrorNotImage');
	});

	it('replaces the staged file on a single-image surface', () => {
		const { stage } = capture({ maxFiles: 1 });

		stage.add([file('first.png')]);
		stage.add([file('second.png')]);

		expect(stage.files.map((f) => f.name)).toEqual(['second.png']);
	});

	it('rejects an add that would exceed the file cap', () => {
		const { stage } = capture({ maxFiles: 2 });

		stage.add([file('a.png'), file('b.png')]);
		stage.add([file('c.png')]);

		expect(stage.files.map((f) => f.name)).toEqual(['a.png', 'b.png']);
		expect(stage.error).toBeNull();
	});

	it('rejects an add that would exceed the total size', () => {
		const { stage } = capture({ maxFiles: 5, totalBytes: 1500 });

		stage.add([file('a.png', 'image/png', 1024)]);
		stage.add([file('b.png', 'image/png', 1024)]);

		expect(stage.files.map((f) => f.name)).toEqual(['a.png']);
		expect(stage.error).toBe('imageTotalTooLarge');
	});

	it('removes the file at the requested index', () => {
		const { stage, onchange } = capture();
		stage.add([file('a.png'), file('b.png')]);

		stage.remove(0);

		expect(stage.files.map((f) => f.name)).toEqual(['b.png']);
		expect(onchange).toHaveBeenLastCalledWith(stage.files);
	});

});

describe('imageUrlErrorMessage', () => {
	it('maps the unreachable code to the unreachable message', () => {
		expect(imageUrlErrorMessage(new ApiError('network down', 'image_url_unreachable', 400))).toBe(
			'imageErrorUrlUnreachable'
		);
	});

	it('maps the not-an-image code to the not-an-image message', () => {
		expect(imageUrlErrorMessage(new ApiError('bad payload', 'image_url_not_image', 400))).toBe(
			'imageErrorUrlNotImage'
		);
	});

	it('falls back to the generic message for an unknown code', () => {
		expect(imageUrlErrorMessage(new ApiError('nope', 'something_else', 400))).toBe(
			'imageErrorUrlGeneric'
		);
	});

	it('falls back to the generic message when there is no code', () => {
		expect(imageUrlErrorMessage(new ApiError('nope', null, 0))).toBe('imageErrorUrlGeneric');
		expect(imageUrlErrorMessage(new Error('boom'))).toBe('imageErrorUrlGeneric');
	});
});
