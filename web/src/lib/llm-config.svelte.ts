/// <reference types="svelte" />

export interface StoredLlmConfig {
	provider: string;
	model: string;
	customBaseUrl: string;
	customApiKey: string;
}

const STORAGE_KEY = 'yummybox-llm-config';

export function readStoredLlmConfig(): StoredLlmConfig | null {
	if (typeof localStorage === 'undefined') return null;
	try {
		const raw = localStorage.getItem(STORAGE_KEY);
		if (!raw) return null;
		const parsed: unknown = JSON.parse(raw);
		if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return null;
		const obj = parsed as Record<string, unknown>;
		return {
			provider: typeof obj.provider === 'string' ? obj.provider : '',
			model: typeof obj.model === 'string' ? obj.model : '',
			customBaseUrl: typeof obj.customBaseUrl === 'string' ? obj.customBaseUrl : '',
			customApiKey: typeof obj.customApiKey === 'string' ? obj.customApiKey : '',
		};
	} catch {
		return null;
	}
}

export function persistLlmConfig(config: StoredLlmConfig): void {
	if (typeof localStorage === 'undefined') return;
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
	} catch {
		// quota exceeded or private mode — silently ignore
	}
}
