import type { TranslationKey } from './i18n/types';
import type { NewIngredientLine } from './types';

type ValidationResult = { ok: true } | { ok: false; field: 'name' | 'ingredients' | 'instructions' | 'portions' | 'source_url'; messageKey: TranslationKey };

export function validateMeal(name: string, ingredients: NewIngredientLine[], instructions: string, portions: number | null = null, sourceUrl: string | null = null): ValidationResult {
	const nameTrim = name.trim();
	if (nameTrim.length === 0) {
		return { ok: false, field: 'name', messageKey: 'validationNameRequired' };
	}
	if (nameTrim.length > 200) {
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
		if (line.name.trim().length > 100) {
			return { ok: false, field: 'ingredients', messageKey: 'validationIngredientNameTooLong' };
		}
		if (line.quantity && line.quantity.length > 50) {
			return { ok: false, field: 'ingredients', messageKey: 'validationIngredientQuantityTooLong' };
		}
	}
	const instructionsTrim = instructions.trim();
	if (instructionsTrim.length === 0) {
		return { ok: false, field: 'instructions', messageKey: 'validationInstructionsRequired' };
	}
	if (instructionsTrim.length > 20000) {
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
		if (!(trimmed.startsWith('http://') || trimmed.startsWith('https://'))) {
			return { ok: false, field: 'source_url', messageKey: 'validationSourceUrlInvalid' };
		}
		if (/\s/.test(trimmed)) {
			return { ok: false, field: 'source_url', messageKey: 'validationSourceUrlInvalid' };
		}
		if (/["'<>`]/.test(trimmed) || /[\x00-\x1F\x7F]/.test(trimmed)) {
			return { ok: false, field: 'source_url', messageKey: 'validationSourceUrlInvalid' };
		}
		const afterScheme = trimmed.startsWith('https://') ? trimmed.slice(8) : trimmed.slice(7);
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
