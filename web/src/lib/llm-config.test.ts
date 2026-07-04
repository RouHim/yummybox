import { describe, it, expect, beforeEach, vi } from 'vitest';
import { readStoredLlmConfig, persistLlmConfig } from './llm-config.svelte';
import type { StoredLlmConfig } from './llm-config.svelte';

const STORAGE_KEY = 'yummybox-llm-config';

// In-memory localStorage polyfill for the Node test environment
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

function makeConfig(overrides?: Partial<StoredLlmConfig>): StoredLlmConfig {
	return {
		provider: 'openai',
		model: 'gpt-4o-mini',
		customBaseUrl: '',
		customApiKey: '',
		...overrides,
	};
}

describe('llm-config persistence', () => {
	beforeEach(() => {
		localStorage.clear();
		vi.clearAllMocks();
	});

	describe('persistLlmConfig', () => {
		it('writes config JSON under the single known key', () => {
			const config = makeConfig();
			persistLlmConfig(config);
			const raw = localStorage.getItem(STORAGE_KEY);
			expect(raw).not.toBeNull();
			const parsed = JSON.parse(raw!);
			expect(parsed).toEqual({
				provider: 'openai',
				model: 'gpt-4o-mini',
				customBaseUrl: '',
				customApiKey: '',
			});
		});

		it('overwrites a previously stored config fully', () => {
			persistLlmConfig(makeConfig({ provider: 'openai', model: 'gpt-4o' }));
			persistLlmConfig(makeConfig({ provider: 'anthropic', model: 'claude-3' }));
			const stored = readStoredLlmConfig();
			expect(stored).toEqual(makeConfig({ provider: 'anthropic', model: 'claude-3' }));
		});

		it('stores custom provider fields', () => {
			persistLlmConfig(makeConfig({
				provider: 'custom',
				model: 'llama3',
				customBaseUrl: 'http://localhost:11434/v1',
				customApiKey: 'sk-custom-key',
			}));
			const stored = readStoredLlmConfig();
			expect(stored).toEqual({
				provider: 'custom',
				model: 'llama3',
				customBaseUrl: 'http://localhost:11434/v1',
				customApiKey: 'sk-custom-key',
			});
		});

		it('stores empty strings for all fields', () => {
			persistLlmConfig(makeConfig({ provider: '', model: '', customBaseUrl: '', customApiKey: '' }));
			const stored = readStoredLlmConfig();
			expect(stored).toEqual({ provider: '', model: '', customBaseUrl: '', customApiKey: '' });
		});

		it('does not throw when localStorage.setItem throws', () => {
			const origSetItem = localStorage.setItem;
			localStorage.setItem = vi.fn(() => { throw new Error('quota'); });
			expect(() => persistLlmConfig(makeConfig())).not.toThrow();
			localStorage.setItem = origSetItem;
		});
	});

	describe('readStoredLlmConfig', () => {
		it('returns null when nothing is stored', () => {
			expect(readStoredLlmConfig()).toBeNull();
		});

		it('returns parsed config from a valid stored entry', () => {
			localStorage.setItem(STORAGE_KEY, JSON.stringify(makeConfig({ provider: 'openai' })));
			const stored = readStoredLlmConfig();
			expect(stored).toEqual(makeConfig({ provider: 'openai' }));
		});

		it('returns empty-string defaults for missing fields in stored JSON', () => {
			localStorage.setItem(STORAGE_KEY, JSON.stringify({ provider: 'openai' }));
			const stored = readStoredLlmConfig();
			expect(stored).toEqual({
				provider: 'openai',
				model: '',
				customBaseUrl: '',
				customApiKey: '',
			});
		});

		it('returns null for malformed JSON', () => {
			localStorage.setItem(STORAGE_KEY, '{not json}');
			expect(readStoredLlmConfig()).toBeNull();
		});

		it('returns null when stored value is not an object', () => {
			localStorage.setItem(STORAGE_KEY, '"just a string"');
			expect(readStoredLlmConfig()).toBeNull();
		});

		it('returns null when stored value is an array', () => {
			localStorage.setItem(STORAGE_KEY, '[]');
			expect(readStoredLlmConfig()).toBeNull();
		});

		it('preserves model field when other fields are missing', () => {
			localStorage.setItem(STORAGE_KEY, JSON.stringify({ model: 'gpt-4o' }));
			const stored = readStoredLlmConfig();
			expect(stored).toEqual({
				provider: '',
				model: 'gpt-4o',
				customBaseUrl: '',
				customApiKey: '',
			});
		});

		it('round-trips a full custom config through persist and read', () => {
			const original = makeConfig({
				provider: 'custom',
				model: 'llama3',
				customBaseUrl: 'https://api.example.com/v1',
				customApiKey: 'sk-secret',
			});
			persistLlmConfig(original);
			const restored = readStoredLlmConfig();
			expect(restored).toEqual(original);
		});

		it('returns null when localStorage throws on getItem', () => {
			const origGetItem = localStorage.getItem;
			localStorage.getItem = vi.fn(() => { throw new Error('denied'); });
			expect(readStoredLlmConfig()).toBeNull();
			localStorage.getItem = origGetItem;
		});
	});
});
