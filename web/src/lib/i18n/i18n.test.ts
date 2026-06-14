// localStorage polyfill for Node test environment
if (typeof localStorage === 'undefined') {

	globalThis.localStorage = (() => {
		let store: Record<string, string> = {};
		return {
			getItem(key: string) { return Object.prototype.hasOwnProperty.call(store, key) ? store[key] : null; },
			setItem(key: string, value: string) { store[key] = value; },
			removeItem(key: string) { delete store[key]; },
			clear() { store = {}; },
			get length() { return Object.keys(store).length; },
			key(index: number) { return Object.keys(store)[index] ?? null; },
		};
	})();
}

import { describe, it, expect, beforeEach } from 'vitest';
import {
	t,
	getLocale,
	setLocale,
	detectInitialLocale,
	initLocale,
	formatNumber,
	formatDate,
	dictionaries,
} from './index';

beforeEach(() => {
	// Reset locale to English before each test
	setLocale('en');
	localStorage.clear();
});

describe('t', () => {
	it('returns English string for English locale', () => {
		setLocale('en');
		expect(t('appTitle')).toBe('Meals');
	});

	it('returns German string for German locale', () => {
		setLocale('de');
		expect(t('appTitle')).toBe('Mahlzeiten');
	});

	it('falls back to English when a key is missing in German', () => {
		const saved = dictionaries.de.validationNameRequired;
		// @ts-expect-error — testing runtime fallback
		delete dictionaries.de.validationNameRequired;
		setLocale('de');
		expect(t('validationNameRequired')).toBe('Name is required');
		dictionaries.de.validationNameRequired = saved;
	});

	it('returns the key string when missing from both dictionaries', () => {
		// @ts-expect-error — testing runtime fallback for nonexistent key
		expect(t('nonexistent')).toBe('nonexistent');
	});

	it('interpolates {name} parameter', () => {
		setLocale('en');
		expect(t('confirmDelete', { name: 'Pasta' })).toBe('Delete "Pasta"?');
	});

	it('interpolates {search} parameter in German', () => {
		setLocale('de');
		expect(t('noResults', { search: 'pizza' })).toBe(
			'Keine Mahlzeiten für \u201Epizza" gefunden. Versuche eine andere Suche.'
		);
	});

	it('replaces missing param with empty string', () => {
		setLocale('en');
		expect(t('noResults', {})).toBe('No meals match "". Try a different search.');
	});
});

describe('setLocale', () => {
	it('updates locale and writes localStorage', () => {
		setLocale('de');
		expect(getLocale()).toBe('de');
		expect(localStorage.getItem('mealme.locale')).toBe('de');
	});

	it('updates locale to en', () => {
		setLocale('de');
		setLocale('en');
		expect(getLocale()).toBe('en');
		expect(localStorage.getItem('mealme.locale')).toBe('en');
	});
});

describe('detectInitialLocale', () => {
	function withNavigator(lang: string | undefined): string {
		const saved = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
		const mockNavigator = lang === undefined ? undefined : { language: lang };
		Object.defineProperty(globalThis, 'navigator', {
			value: mockNavigator,
			configurable: true,
			writable: true,
		});
		const result = detectInitialLocale();
		if (saved) {
			Object.defineProperty(globalThis, 'navigator', saved);
		} else {
			delete (globalThis as Record<string, unknown>).navigator;
		}
		return result;
	}

	it('returns de for de', () => {
		expect(withNavigator('de')).toBe('de');
	});

	it('returns de for de-DE', () => {
		expect(withNavigator('de-DE')).toBe('de');
	});

	it('returns de for de-AT', () => {
		expect(withNavigator('de-AT')).toBe('de');
	});

	it('returns de for de-CH', () => {
		expect(withNavigator('de-CH')).toBe('de');
	});

	it('returns en for en', () => {
		expect(withNavigator('en')).toBe('en');
	});

	it('returns en for fr', () => {
		expect(withNavigator('fr')).toBe('en');
	});

	it('returns en for es', () => {
		expect(withNavigator('es')).toBe('en');
	});

	it('returns en for empty string', () => {
		expect(withNavigator('')).toBe('en');
	});

	it('returns en when navigator is undefined', () => {
		expect(withNavigator(undefined)).toBe('en');
	});
});

describe('initLocale', () => {
	it('restores de from localStorage', () => {
		localStorage.setItem('mealme.locale', 'de');
		initLocale();
		expect(getLocale()).toBe('de');
	});

	it('falls back to detectInitialLocale when localStorage is empty', () => {
		localStorage.clear();
		initLocale();
		expect(getLocale()).toBe('en');
	});

	it('falls back to detectInitialLocale for invalid localStorage value', () => {
		localStorage.setItem('mealme.locale', 'fr');
		initLocale();
		expect(getLocale()).toBe('en');
	});
});

describe('formatNumber', () => {
	it('formats number in English locale', () => {
		setLocale('en');
		expect(formatNumber(1234.56)).toBe('1,234.56');
	});

	it('formats number in German locale', () => {
		setLocale('de');
		expect(formatNumber(1234.56)).toBe('1.234,56');
	});
});

describe('formatDate', () => {
	it('formats date in German locale', () => {
		setLocale('de');
		const result = formatDate(new Date('2026-06-13T12:00:00Z'), {
			day: '2-digit',
			month: '2-digit',
			year: 'numeric',
		});
		expect(result).toBe('13.06.2026');
	});

	it('formats date in English locale', () => {
		setLocale('en');
		const result = formatDate(new Date('2026-06-13T12:00:00Z'), {
			day: '2-digit',
			month: '2-digit',
			year: 'numeric',
		});
		expect(result).toBe('06/13/2026');
	});

	it('accepts ISO string input', () => {
		setLocale('de');
		const result = formatDate('2026-06-13', {
			year: 'numeric',
			month: 'long',
			day: 'numeric',
		});
		expect(result).toBe('13. Juni 2026');
	});
});
