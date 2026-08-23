import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { theme, setTheme, cycleTheme, initTheme } from './theme.svelte';

const STORAGE_KEY = 'yummybox-theme';

// In-memory localStorage polyfill for the Node test environment.
// The static import above evaluates `readStored()` with localStorage
// undefined, so module-load defaults to 'system'; the load-time read is
// exercised separately via vi.resetModules() + dynamic import below.
let _store: Record<string, string> = {};
const polyfillStorage: Storage = {
	getItem: vi.fn((key: string) => _store[key] ?? null),
	setItem: vi.fn((key: string, value: string) => { _store[key] = value; }),
	removeItem: vi.fn((key: string) => { delete _store[key]; }),
	clear: vi.fn(() => { _store = {}; }),
	key: vi.fn((_index: number) => null),
	get length() { return Object.keys(_store).length; },
};
globalThis.localStorage = polyfillStorage;

const element = { setAttribute: vi.fn(), removeAttribute: vi.fn() };

function stubDocument(): void {
	vi.stubGlobal('document', { documentElement: element });
}

beforeEach(() => {
	localStorage.clear();
	vi.clearAllMocks();
});

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('theme', () => {
	it('initializes to system with empty storage', () => {
		expect(theme.current).toBe('system');
	});
});

describe('setTheme', () => {
	it('updates the current theme', () => {
		setTheme('dark');
		expect(theme.current).toBe('dark');
	});

	it('persists the theme to localStorage', () => {
		setTheme('light');
		expect(_store[STORAGE_KEY]).toBe('light');
	});

	it('sets the data-theme attribute on the document root', () => {
		stubDocument();
		setTheme('dark');
		expect(element.setAttribute).toHaveBeenCalledWith('data-theme', 'dark');
	});

	it('removes the data-theme attribute when set to system', () => {
		stubDocument();
		setTheme('system');
		expect(element.removeAttribute).toHaveBeenCalledWith('data-theme');
	});
});

describe('cycleTheme', () => {
	it('cycles system -> light -> dark -> system', () => {
		setTheme('system');
		expect(cycleTheme()).toBe('light');
		expect(cycleTheme()).toBe('dark');
		expect(cycleTheme()).toBe('system');
	});

	it('persists each step of the cycle', () => {
		setTheme('system');
		cycleTheme();
		expect(_store[STORAGE_KEY]).toBe('light');
		cycleTheme();
		expect(_store[STORAGE_KEY]).toBe('dark');
	});
});

describe('initTheme', () => {
	it('applies the current theme to the document root', () => {
		stubDocument();
		setTheme('light');
		element.setAttribute.mockClear();
		initTheme();
		expect(element.setAttribute).toHaveBeenCalledWith('data-theme', 'light');
	});
});

describe('theme module load', () => {
	it('reads a stored theme when the module loads', async () => {
		vi.resetModules();
		_store[STORAGE_KEY] = 'dark';
		const loaded = await import('./theme.svelte');
		expect(loaded.theme.current).toBe('dark');
	});

	it('falls back to system for an invalid stored value', async () => {
		vi.resetModules();
		_store[STORAGE_KEY] = 'neon';
		const loaded = await import('./theme.svelte');
		expect(loaded.theme.current).toBe('system');
	});
});
