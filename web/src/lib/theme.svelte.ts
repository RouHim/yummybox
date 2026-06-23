/// <reference types="svelte" />

export type Theme = 'light' | 'dark' | 'system';

const STORAGE_KEY = 'mealme-theme';

function readStored(): Theme {
	if (typeof localStorage === 'undefined') return 'system';
	const stored = localStorage.getItem(STORAGE_KEY);
	return stored === 'light' || stored === 'dark' || stored === 'system' ? stored : 'system';
}

export const theme = $state<{ current: Theme }>({ current: readStored() });

function persist(): void {
	if (typeof localStorage !== 'undefined') {
		localStorage.setItem(STORAGE_KEY, theme.current);
	}
}

function applyToDOM(): void {
	if (typeof document === 'undefined') return;
	if (theme.current === 'system') {
		document.documentElement.removeAttribute('data-theme');
	} else {
		document.documentElement.setAttribute('data-theme', theme.current);
	}
}

export function setTheme(t: Theme): void {
	theme.current = t;
	applyToDOM();
	persist();
}

export function cycleTheme(): Theme {
	const order: Theme[] = ['system', 'light', 'dark'];
	const idx = order.indexOf(theme.current);
	setTheme(order[(idx + 1) % order.length]);
	return theme.current;
}

export function initTheme(): void {
	applyToDOM();
}
