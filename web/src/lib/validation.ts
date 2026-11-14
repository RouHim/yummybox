import type { TranslationKey } from './i18n/types';
import type { NewIngredientLine } from './types';

type ValidationResult = { ok: true } | { ok: false; field: 'name' | 'ingredients' | 'instructions' | 'portions' | 'source_url'; messageKey: TranslationKey };

/**
 * UTF-8 byte length. The backend measures with `str::len()`, so a multi-byte
 * character must count the same way here or the limits drift apart.
 */
function utf8Length(value: string): number {
	return new TextEncoder().encode(value).length;
}

/** Unicode White_Space, as Rust's `char::is_whitespace` sees it (no U+FEFF). */
const WHITESPACE = /[\t\n\v\f\r \u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]/;
/** Unicode Cc controls, as Rust's `char::is_control` sees them. */
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;

export function validateMeal(name: string, ingredients: NewIngredientLine[], instructions: string, portions: number | null = null, sourceUrl: string | null = null): ValidationResult {
	const nameTrim = name.trim();
	if (nameTrim.length === 0) {
		return { ok: false, field: 'name', messageKey: 'validationNameRequired' };
	}
	if (utf8Length(nameTrim) > 200) {
		return { ok: false, field: 'name', messageKey: 'validationNameTooLong' };
	}
	if (ingredients.length === 0) {
		return { ok: false, field: 'ingredients', messageKey: 'validationIngredientsRequired' };
	}
	if (ingredients.length > 100) {
		return { ok: false, field: 'ingredients', messageKey: 'validationTooManyIngredients' };
	}
	for (let i = 0; i < ingredients.length; i++) {
		const line = ingredients[i];
		if (line.name.trim().length === 0) {
			return { ok: false, field: 'ingredients', messageKey: 'validationIngredientNameRequired' };
		}
		if (utf8Length(line.name.trim()) > 100) {
			return { ok: false, field: 'ingredients', messageKey: 'validationIngredientNameTooLong' };
		}
		if (line.quantity && utf8Length(line.quantity) > 50) {
			return { ok: false, field: 'ingredients', messageKey: 'validationIngredientQuantityTooLong' };
		}
	}
	const instructionsTrim = instructions.trim();
	if (instructionsTrim.length === 0) {
		return { ok: false, field: 'instructions', messageKey: 'validationInstructionsRequired' };
	}
	if (utf8Length(instructionsTrim) > 20000) {
		return { ok: false, field: 'instructions', messageKey: 'validationInstructionsTooLong' };
	}
	if (portions != null && (portions <= 0 || portions > 10000)) {
		return { ok: false, field: 'portions', messageKey: 'validationPortionsInvalid' };
	}
	if (sourceUrl != null && sourceUrl.trim().length > 0) {
		const trimmed = sourceUrl.trim();
		if ([...trimmed].length > 2048) {
			return { ok: false, field: 'source_url', messageKey: 'validationSourceUrlTooLong' };
		}
		const scheme = trimmed.slice(0, 8).toLowerCase();
		if (!(scheme.startsWith('http://') || scheme.startsWith('https://'))) {
			return { ok: false, field: 'source_url', messageKey: 'validationSourceUrlInvalid' };
		}
		if (WHITESPACE.test(trimmed)) {
			return { ok: false, field: 'source_url', messageKey: 'validationSourceUrlInvalid' };
		}
		if (/["'<>`]/.test(trimmed) || CONTROL.test(trimmed)) {
			return { ok: false, field: 'source_url', messageKey: 'validationSourceUrlInvalid' };
		}
		const afterScheme = scheme.startsWith('https://') ? trimmed.slice(8) : trimmed.slice(7);
		if (!afterScheme) {
			return { ok: false, field: 'source_url', messageKey: 'validationSourceUrlInvalid' };
		}
		const hostPart = afterScheme.split(/[\/?#]/)[0];
		if (!hostPart) {
			return { ok: false, field: 'source_url', messageKey: 'validationSourceUrlInvalid' };
		}
	}
	return { ok: true };
}
