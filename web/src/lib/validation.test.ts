import { describe, it, expect } from 'vitest';
import { validateMeal } from './validation';

function longString(n: number): string {
	return Array.from({ length: n }, () => 'a').join('');
}

describe('validateMeal', () => {
	it('returns ok for valid input', () => {
		expect(validateMeal('Pasta', 'noodles')).toEqual({ ok: true });
	});

	it('rejects empty name', () => {
		expect(validateMeal('', 'x')).toMatchObject({ ok: false, field: 'name' });
	});

	it('rejects whitespace-only name', () => {
		expect(validateMeal('   ', 'x')).toMatchObject({ ok: false, field: 'name' });
	});

	it('accepts name at exactly 200 chars', () => {
		expect(validateMeal(longString(200), 'x')).toEqual({ ok: true });
	});

	it('rejects name at 201 chars', () => {
		expect(validateMeal(longString(201), 'x')).toMatchObject({ ok: false, field: 'name' });
	});

	it('rejects empty ingredients', () => {
		expect(validateMeal('x', '')).toMatchObject({ ok: false, field: 'ingredients' });
	});

	it('rejects whitespace-only ingredients', () => {
		expect(validateMeal('x', '   ')).toMatchObject({ ok: false, field: 'ingredients' });
	});

	it('accepts ingredients at exactly 5000 chars', () => {
		expect(validateMeal('x', longString(5000))).toEqual({ ok: true });
	});

	it('rejects ingredients at 5001 chars', () => {
		expect(validateMeal('x', longString(5001))).toMatchObject({ ok: false, field: 'ingredients' });
	});
});
