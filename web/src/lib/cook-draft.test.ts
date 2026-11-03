import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
	COOK_DRAFT_KEY,
	clearCookDraft,
	readCookDraft,
	writeCookDraft,
	type CookDraft,
} from './cook-draft';

// In-memory sessionStorage polyfill for the Node test environment.
let store: Record<string, string> = {};
const polyfillStorage: Storage = {
	getItem: vi.fn((key: string) => store[key] ?? null),
	setItem: vi.fn((key: string, value: string) => {
		store[key] = value;
	}),
	removeItem: vi.fn((key: string) => {
		delete store[key];
	}),
	clear: vi.fn(() => {
		store = {};
	}),
	key: vi.fn((_index: number) => null),
	get length() {
		return Object.keys(store).length;
	},
};
globalThis.sessionStorage = polyfillStorage;

const draft: CookDraft = {
	name: 'Gemüsepfanne',
	ingredients: [
		{ name: 'Paprika', quantity: '2' },
		{ name: 'Salz', quantity: null },
	],
	instructions: 'Alles anbraten.',
	portions: 3,
	imageDataUrl: 'data:image/jpeg;base64,AAAA',
	sourceUrl: 'https://example.com/recipe',
};

beforeEach(() => {
	sessionStorage.clear();
	vi.clearAllMocks();
});

describe('cook draft', () => {
	it('round-trips every field through storage', () => {
		writeCookDraft(draft);

		expect(readCookDraft()).toEqual(draft);
	});

	it('stores one spelling of the source URL', () => {
		writeCookDraft(draft);

		const raw = sessionStorage.getItem(COOK_DRAFT_KEY) ?? '';
		expect(raw).toContain('"sourceUrl"');
		expect(raw).not.toContain('source_url');
	});

	it('ignores a draft written by an older schema version', () => {
		sessionStorage.setItem(
			COOK_DRAFT_KEY,
			JSON.stringify({ version: 0, ...draft })
		);

		expect(readCookDraft()).toBeNull();
	});

	it('ignores unparseable storage content', () => {
		sessionStorage.setItem(COOK_DRAFT_KEY, '{not json');

		expect(readCookDraft()).toBeNull();
	});

	it('ignores a draft whose ingredients do not have the expected shape', () => {
		sessionStorage.setItem(
			COOK_DRAFT_KEY,
			JSON.stringify({ version: 1, ...draft, ingredients: [{ name: 42 }] })
		);

		expect(readCookDraft()).toBeNull();
	});

	it('returns null when nothing was stored', () => {
		expect(readCookDraft()).toBeNull();
	});

	it('propagates a storage rejection so the caller can show it', () => {
		const setItem = vi.mocked(sessionStorage.setItem);
		setItem.mockImplementationOnce(() => {
			throw new Error('QuotaExceededError');
		});

		expect(() => writeCookDraft(draft)).toThrow('QuotaExceededError');
	});

	it('clears the stored draft', () => {
		writeCookDraft(draft);
		clearCookDraft();

		expect(readCookDraft()).toBeNull();
	});
});
