import { describe, it, expect } from 'vitest';
import { validateMeal } from './validation';
import type { NewIngredientLine } from './types';

function longString(n: number): string {
	return Array.from({ length: n }, () => 'a').join('');
}

function ingr(name: string, quantity?: string): NewIngredientLine {
	return { name, quantity: quantity ?? null };
}

describe('validateMeal', () => {
	it('returns ok for valid name and one ingredient line', () => {
		expect(validateMeal('Pasta', [ingr('noodles')], 'test instructions')).toEqual({ ok: true });
	});

	it('rejects empty name', () => {
		expect(validateMeal('', [ingr('x')], 'test instructions')).toMatchObject({ ok: false, field: 'name' });
	});

	it('rejects whitespace-only name', () => {
		expect(validateMeal('   ', [ingr('x')], 'test instructions')).toMatchObject({ ok: false, field: 'name' });
	});

	it('accepts name at exactly 200 chars', () => {
		expect(validateMeal(longString(200), [ingr('x')], 'test instructions')).toEqual({ ok: true });
	});

	it('rejects name at 201 chars', () => {
		expect(validateMeal(longString(201), [ingr('x')], 'test instructions')).toMatchObject({ ok: false, field: 'name' });
	});

	it('rejects no ingredient lines', () => {
		expect(validateMeal('x', [], 'test instructions')).toMatchObject({ ok: false, field: 'ingredients' });
	});

	it('rejects ingredient line with empty name', () => {
		expect(validateMeal('x', [ingr('   ')], 'test instructions')).toMatchObject({ ok: false, field: 'ingredients' });
	});

	it('rejects ingredient name above 100 chars', () => {
		expect(validateMeal('x', [ingr(longString(101))], 'test instructions')).toMatchObject({ ok: false, field: 'ingredients' });
	});

	it('rejects ingredient quantity above 50 chars', () => {
		expect(validateMeal('x', [ingr('valid', longString(51))], 'test instructions')).toMatchObject({ ok: false, field: 'ingredients' });
	});

	it('rejects above 100 ingredient lines', () => {
		const lines = Array.from({ length: 101 }, (_, i) => ingr(`ingredient ${i}`));
		expect(validateMeal('x', lines, 'test instructions')).toMatchObject({ ok: false, field: 'ingredients' });
	});

	it('rejects empty instructions', () => {
		expect(validateMeal('x', [ingr('noodles')], '')).toMatchObject({ ok: false, field: 'instructions' });
	});

	it('rejects instructions over 20000 chars', () => {
		expect(validateMeal('x', [ingr('noodles')], longString(20001))).toMatchObject({ ok: false, field: 'instructions' });
	});
});

describe('validateMeal sourceUrl', () => {
	it('accepts null', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, null)).toEqual({ ok: true });
	});

	it('accepts an empty string', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, '')).toEqual({ ok: true });
	});

	it('accepts a whitespace-only value', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, '   ')).toEqual({ ok: true });
	});

	it('accepts https URLs', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, 'https://example.com/recipe/1')).toEqual({ ok: true });
	});

	it('accepts http URLs', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, 'http://example.com/recipe')).toEqual({ ok: true });
	});

	it('rejects a scheme-relative URL', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, '//example.com/recipe')).toMatchObject({
			ok: false,
			field: 'source_url',
			messageKey: 'validationSourceUrlInvalid',
		});
	});

	it('rejects a javascript: URL', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, 'javascript:alert(1)')).toMatchObject({
			ok: false,
			field: 'source_url',
		});
	});

	it('rejects a data: URL', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, 'data:text/html,<script>')).toMatchObject({
			ok: false,
			field: 'source_url',
		});
	});

	it('rejects a URL whose host is missing', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, 'https://')).toMatchObject({ ok: false, field: 'source_url' });
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, 'https:///recipe')).toMatchObject({ ok: false, field: 'source_url' });
	});

	it('rejects URLs containing whitespace', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, 'https://example.com/a b')).toMatchObject({
			ok: false,
			field: 'source_url',
		});
	});

	it('accepts a URL at exactly 2048 characters', () => {
		const prefix = 'https://example.com/';
		const url = prefix + longString(2048 - prefix.length);
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, url)).toEqual({ ok: true });
	});

	it('rejects a URL at 2049 characters', () => {
		const prefix = 'https://example.com/';
		const url = prefix + longString(2049 - prefix.length);
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, url)).toMatchObject({
			ok: false,
			field: 'source_url',
			messageKey: 'validationSourceUrlTooLong',
		});
	});

	it('accepts an upper-case scheme, matching the backend', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, 'HTTPS://Example.com/Recipe')).toEqual({ ok: true });
	});

	it('rejects a C1 control character, matching the backend', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, 'https://example.com/\u0085')).toMatchObject({
			ok: false,
			field: 'source_url',
		});
	});

	it('does not treat a byte-order mark as whitespace, matching the backend', () => {
		expect(validateMeal('x', [ingr('noodles')], 'steps', null, 'https://example.com/\ufeff')).toEqual({ ok: true });
	});
});
