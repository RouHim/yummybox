import type { Locale, LocalePreference, TranslationKey } from './types';
import { en } from './en';
import { de } from './de';

export const dictionaries: Record<Locale, Record<TranslationKey, string>> = { en, de };

const STORAGE_KEY = 'yummybox-locale';

let _locale = $state<Locale>('en');
let _preference = $state<LocalePreference>('system');

export function getLocale(): Locale {
	return _locale;
}

export function getLocalePreference(): LocalePreference {
	return _preference;
}

export function t(key: TranslationKey, params?: Record<string, string | undefined>): string {
	const dict = dictionaries[_locale] ?? dictionaries.en;
	let value = dict[key];
	if (value === undefined) {
		value = dictionaries.en[key] ?? key;
	}
	if (params) {
		value = value.replace(/\{(\w+)\}/g, (_, k: string) => String(params[k] ?? ''));
	}
	return value;
}

export function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
	return new Intl.NumberFormat(_locale, options).format(value);
}

export function formatDate(value: Date | string | number, options?: Intl.DateTimeFormatOptions): string {
	const date = value instanceof Date ? value : new Date(value);
	return new Intl.DateTimeFormat(_locale, options).format(date);
}

export function detectInitialLocale(): Locale {
	if (typeof navigator === 'undefined') return 'en';
	return navigator.language.toLowerCase().startsWith('de') ? 'de' : 'en';
}

export function setLocale(pref: LocalePreference): void {
	_preference = pref;
	_locale = pref === 'system' ? detectInitialLocale() : pref;
	if (typeof localStorage !== 'undefined') {
		try {
			localStorage.setItem(STORAGE_KEY, pref);
		} catch {
			// quota exceeded or private mode — silently ignore
		}
	}
}

export function initLocale(): void {
	if (typeof localStorage === 'undefined') {
		_preference = 'system';
		_locale = detectInitialLocale();
		return;
	}
	const stored = localStorage.getItem(STORAGE_KEY);
	if (stored === 'en' || stored === 'de') {
		_preference = stored;
		_locale = stored;
	} else {
		// 'system', null, invalid, or corrupted — resolve via navigator; do NOT write
		_preference = 'system';
		_locale = detectInitialLocale();
	}
}
