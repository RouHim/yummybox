import type { TranslationKey } from './i18n/types';

type ValidationResult = { ok: true } | { ok: false; field: 'name' | 'ingredients'; messageKey: TranslationKey };

export function validateMeal(name: string, ingredients: string): ValidationResult {
	const nameTrim = name.trim();
	if (nameTrim.length === 0) {
		return { ok: false, field: 'name', messageKey: 'validationNameRequired' };
	}
	if (nameTrim.length > 200) {
		return { ok: false, field: 'name', messageKey: 'validationNameTooLong' };
	}
	const ingTrim = ingredients.trim();
	if (ingTrim.length === 0) {
		return { ok: false, field: 'ingredients', messageKey: 'validationIngredientsRequired' };
	}
	if (ingTrim.length > 5000) {
		return { ok: false, field: 'ingredients', messageKey: 'validationIngredientsTooLong' };
	}
	return { ok: true };
}
