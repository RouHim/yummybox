import type { TranslationKey } from './i18n/types';
import type { NewIngredientLine } from './types';

type ValidationResult = { ok: true } | { ok: false; field: 'name' | 'ingredients' | 'instructions' | 'portions'; messageKey: TranslationKey };

export function validateMeal(name: string, ingredients: NewIngredientLine[], instructions: string, portions: number | null = null): ValidationResult {
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
	return { ok: true };
}
