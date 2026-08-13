import type { TranslationKey } from './i18n';

export const MAX_GENERATE_IMAGES = 5;
export const MAX_GENERATE_IMAGE_BYTES = 20 * 1024 * 1024;
// Below the backend body limit (DefaultBodyLimit::max(MAX_BODY_BYTES), 50 MB):
// the multipart framing and the model/ingredients text fields also count toward
// the body limit, so a set summing to exactly 50 MB would be rejected with 413.
export const MAX_GENERATE_TOTAL_BYTES = 49 * 1024 * 1024;
// Mirrors the backend allowlist in src/import.rs (generate_meal).
export const GENERATE_IMAGE_MIME_TYPES = [
	'image/jpeg',
	'image/png',
	'image/webp',
	'image/gif',
] as const;

/**
 * Returns a TranslationKey describing why `file` is rejected, or null if it
 * may be uploaded. Mirrors the backend limits (image/jpeg, image/png,
 * image/webp, image/gif and 20 MB per file).
 */
export function validateGenerateImage(file: File): TranslationKey | null {
	if (!GENERATE_IMAGE_MIME_TYPES.some((type) => type === file.type)) {
		return 'generateErrorUnsupportedImageType';
	}
	if (file.size > MAX_GENERATE_IMAGE_BYTES) return 'generateErrorImageTooLarge';
	return null;
}

/**
 * Returns a TranslationKey when the combined size of `files` would exceed the
 * backend body limit, or null if the set may be uploaded.
 */
export function validateGenerateImageTotal(files: File[]): TranslationKey | null {
	if (files.reduce((sum, f) => sum + f.size, 0) > MAX_GENERATE_TOTAL_BYTES) {
		return 'imageTotalTooLarge';
	}
	return null;
}
