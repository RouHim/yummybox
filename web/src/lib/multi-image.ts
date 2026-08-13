import type { TranslationKey } from './i18n';

export const MAX_GENERATE_IMAGES = 5;
export const MAX_GENERATE_IMAGE_BYTES = 20 * 1024 * 1024;

/**
 * Returns a TranslationKey describing why `file` is rejected, or null if it
 * may be uploaded. Mirrors the backend limits (image/* and 20 MB).
 */
export function validateGenerateImage(file: File): TranslationKey | null {
	if (!file.type.startsWith('image/')) return 'generateErrorNotImage';
	if (file.size > MAX_GENERATE_IMAGE_BYTES) return 'generateErrorImageTooLarge';
	return null;
}
