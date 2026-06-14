import type { Locale, TranslationKey } from './types';
import { en } from './en';
import { de } from './de';

export const dictionaries: Record<Locale, Record<TranslationKey, string>> = { en, de };

let _locale = $state<Locale>('en');

export function getLocale(): Locale {
	return _locale;
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

export function initLocale(): void {
	_locale = detectInitialLocale();
}
