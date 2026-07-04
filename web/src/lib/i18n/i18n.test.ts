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
	getLocalePreference,
	detectInitialLocale,
	initLocale,
	setLocale,
	formatNumber,
	formatDate,
	dictionaries,
} from './index';

beforeEach(() => {
	localStorage.clear();
});

function withLocale(lang: 'en' | 'de'): void {
	const saved = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
	Object.defineProperty(globalThis, 'navigator', {
		value: { language: lang },
		configurable: true,
		writable: true,
	});
	initLocale();
	if (saved) {
		Object.defineProperty(globalThis, 'navigator', saved);
	} else {
		delete (globalThis as Record<string, unknown>).navigator;
	}
}

describe('t', () => {
	it('returns English string for English locale', () => {
		withLocale('en');
		expect(t('appTitle')).toBe('YummyBox');
	});

	it('returns German string for German locale', () => {
		withLocale('de');
		expect(t('appTitle')).toBe('YummyBox');
	});

	it('falls back to English when a key is missing in German', () => {
		const saved = dictionaries.de.validationNameRequired;
		// @ts-expect-error — testing runtime fallback
		delete dictionaries.de.validationNameRequired;
		withLocale('de');
		expect(t('validationNameRequired')).toBe('Name is required');
		dictionaries.de.validationNameRequired = saved;
	});

	it('returns the key string when missing from both dictionaries', () => {
		// @ts-expect-error — testing runtime fallback for nonexistent key
		expect(t('nonexistent')).toBe('nonexistent');
	});

	it('interpolates {name} parameter', () => {
		withLocale('en');
		expect(t('confirmDelete', { name: 'Pasta' })).toBe('Delete "Pasta"?');
	});

	it('interpolates {search} parameter in German', () => {
		withLocale('de');
		expect(t('noResults', { search: 'pizza' })).toBe(
			'Keine Mahlzeiten für \u201Epizza" gefunden. Versuche eine andere Suche.'
		);
	});

	it('replaces missing param with empty string', () => {
		withLocale('en');
		expect(t('noResults', {})).toBe('No meals match "". Try a different search.');
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
	it('given_navigator_is_de_then_init_locale_uses_de', () => {
		const saved = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
		Object.defineProperty(globalThis, 'navigator', {
			value: { language: 'de' },
			configurable: true,
			writable: true,
		});
		initLocale();
		expect(getLocale()).toBe('de');
		if (saved) {
			Object.defineProperty(globalThis, 'navigator', saved);
		} else {
			delete (globalThis as Record<string, unknown>).navigator;
		}
	});

	it('given_navigator_is_en_then_init_locale_uses_en', () => {
		initLocale();
		expect(getLocale()).toBe('en');
	});

	it('given_stored_yummybox_locale_is_de_then_initLocale_uses_de', () => {
		localStorage.setItem('yummybox-locale', 'de');
		initLocale();
		expect(getLocale()).toBe('de');
		expect(getLocalePreference()).toBe('de');
	});

	it('given_stored_yummybox_locale_is_system_and_navigator_de_then_initLocale_uses_de', () => {
		localStorage.setItem('yummybox-locale', 'system');
		const saved = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
		Object.defineProperty(globalThis, 'navigator', {
			value: { language: 'de-DE' },
			configurable: true,
			writable: true,
		});
		initLocale();
		expect(getLocale()).toBe('de');
		expect(getLocalePreference()).toBe('system');
		expect(localStorage.getItem('yummybox-locale')).toBe('system');
		if (saved) {
			Object.defineProperty(globalThis, 'navigator', saved);
		} else {
			delete (globalThis as Record<string, unknown>).navigator;
		}
	});

	it('given_stored_yummybox_locale_is_null_then_initLocale_uses_navigator_and_does_not_write', () => {
		localStorage.removeItem('yummybox-locale');
		initLocale();
		expect(getLocale()).toBe('en');
		expect(localStorage.getItem('yummybox-locale')).toBeNull();
	});

	it('given_stored_yummybox_locale_is_invalid_then_initLocale_falls_back_and_does_not_write', () => {
		localStorage.setItem('yummybox-locale', 'fr');
		initLocale();
		expect(getLocale()).toBe('en');
		expect(localStorage.getItem('yummybox-locale')).toBe('fr');
	});
});

describe('setLocale', () => {
	it('given_no_stored_preference_when_setLocale_de_then_locale_is_de_and_stored', () => {
		localStorage.clear();
		setLocale('de');
		expect(getLocale()).toBe('de');
		expect(localStorage.getItem('yummybox-locale')).toBe('de');
	});

	it('given_setLocale_system_then_locale_resolves_via_navigator_and_system_stored', () => {
		const saved = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
		Object.defineProperty(globalThis, 'navigator', {
			value: { language: 'de-DE' },
			configurable: true,
			writable: true,
		});
		setLocale('system');
		expect(getLocale()).toBe('de');
		expect(localStorage.getItem('yummybox-locale')).toBe('system');
		if (saved) {
			Object.defineProperty(globalThis, 'navigator', saved);
		} else {
			delete (globalThis as Record<string, unknown>).navigator;
		}
	});

	it('given_setLocale_does_not_throw_when_setItem_throws', () => {
		const orig = localStorage.setItem;
		localStorage.setItem = () => { throw new Error('quota'); };
		expect(() => setLocale('en')).not.toThrow();
		localStorage.setItem = orig;
	});
});

describe('formatNumber', () => {
	it('formats number in English locale', () => {
		withLocale('en');
		expect(formatNumber(1234.56)).toBe('1,234.56');
	});

	it('formats number in German locale', () => {
		withLocale('de');
		expect(formatNumber(1234.56)).toBe('1.234,56');
	});
});

describe('formatDate', () => {
	it('formats date in German locale', () => {
		withLocale('de');
		const result = formatDate(new Date('2026-06-13T12:00:00Z'), {
			day: '2-digit',
			month: '2-digit',
			year: 'numeric',
		});
		expect(result).toBe('13.06.2026');
	});

	it('formats date in English locale', () => {
		withLocale('en');
		const result = formatDate(new Date('2026-06-13T12:00:00Z'), {
			day: '2-digit',
			month: '2-digit',
			year: 'numeric',
		});
		expect(result).toBe('06/13/2026');
	});

	it('accepts ISO string input', () => {
		withLocale('de');
		const result = formatDate('2026-06-13', {
			year: 'numeric',
			month: 'long',
			day: 'numeric',
		});
		expect(result).toBe('13. Juni 2026');
	});
});

describe('dictionary key parity', () => {
	it('has identical keys across en and de dictionaries', () => {
		const enKeys = Object.keys(dictionaries.en).sort();
		const deKeys = Object.keys(dictionaries.de).sort();
		expect(enKeys).toEqual(deKeys);
	});
});
