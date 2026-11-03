import type { NewIngredientLine } from './types';

/**
 * The ephemeral handoff between the spontaneous-cooking flow and the cooking
 * view: written by `/spontaneous`, read by `/spontaneous/cook`.
 *
 * Both routes use this module, so the storage key, the payload shape and the
 * image budget live in one place instead of being mirrored on each side.
 */
export const COOK_DRAFT_KEY = 'yummybox-cook-draft';

/** Bumped when the payload shape changes; older drafts are ignored. */
const COOK_DRAFT_VERSION = 1;

/**
 * Longest edge of the stored draft image. Generated JPEGs and user photos
 * routinely exceed the ~5 MB sessionStorage quota as base64 data URLs, so the
 * image is re-encoded small enough to fit.
 */
const COOK_DRAFT_MAX_EDGE = 1280;

export interface CookDraft {
	name: string;
	ingredients: NewIngredientLine[];
	instructions: string;
	portions: number | null;
	imageDataUrl: string | null;
	sourceUrl: string | null;
}

interface StoredCookDraft extends CookDraft {
	version: number;
}

/** Re-encode an image as a downscaled JPEG data URL that fits in storage. */
export function encodeDraftImage(file: File): Promise<string> {
	const { promise, resolve, reject } = Promise.withResolvers<string>();
	const url = URL.createObjectURL(file);
	const img = new Image();
	img.onload = () => {
		URL.revokeObjectURL(url);
		const scale = Math.min(
			1,
			COOK_DRAFT_MAX_EDGE / Math.max(img.naturalWidth, img.naturalHeight)
		);
		const canvas = document.createElement('canvas');
		canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
		canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
		const ctx = canvas.getContext('2d');
		if (!ctx) {
			reject(new Error('could not create canvas context'));
			return;
		}
		// Flatten transparency onto white so JPEG re-encoding keeps a clean background.
		ctx.fillStyle = '#fff';
		ctx.fillRect(0, 0, canvas.width, canvas.height);
		ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
		resolve(canvas.toDataURL('image/jpeg', 0.8));
	};
	img.onerror = () => {
		URL.revokeObjectURL(url);
		reject(new Error('could not load image'));
	};
	img.src = url;
	return promise;
}

/** Persist a draft. Throws when storage rejects the write (quota, privacy mode). */
export function writeCookDraft(draft: CookDraft): void {
	const payload: StoredCookDraft = { version: COOK_DRAFT_VERSION, ...draft };
	sessionStorage.setItem(COOK_DRAFT_KEY, JSON.stringify(payload));
}

export function clearCookDraft(): void {
	sessionStorage.removeItem(COOK_DRAFT_KEY);
}

function isIngredientLine(value: unknown): value is NewIngredientLine {
	if (typeof value !== 'object' || value === null) return false;
	const line = value as NewIngredientLine;
	return typeof line.name === 'string' && (line.quantity === null || typeof line.quantity === 'string');
}

/** Read a draft, or `null` when none is stored, unreadable or outdated. */
export function readCookDraft(): CookDraft | null {
	let raw: string | null;
	try {
		raw = sessionStorage.getItem(COOK_DRAFT_KEY);
	} catch {
		return null;
	}
	if (!raw) return null;

	let parsed: unknown;
	try {
		parsed = JSON.parse(raw);
	} catch {
		return null;
	}
	if (typeof parsed !== 'object' || parsed === null) return null;

	const draft = parsed as StoredCookDraft;
	if (draft.version !== COOK_DRAFT_VERSION) return null;
	if (typeof draft.name !== 'string' || typeof draft.instructions !== 'string') return null;
	if (!Array.isArray(draft.ingredients) || !draft.ingredients.every(isIngredientLine)) return null;

	return {
		name: draft.name,
		ingredients: draft.ingredients.map((line) => ({ name: line.name, quantity: line.quantity })),
		instructions: draft.instructions,
		portions: typeof draft.portions === 'number' ? draft.portions : null,
		imageDataUrl: typeof draft.imageDataUrl === 'string' ? draft.imageDataUrl : null,
		sourceUrl: typeof draft.sourceUrl === 'string' ? draft.sourceUrl : null,
	};
}
