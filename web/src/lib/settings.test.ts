import { describe, it, expect, vi } from 'vitest';
import {
	SettingsCommitter,
	isAiConfigured,
	type SettingsSnapshot,
} from './settings.svelte';

function snapshot(overrides: Partial<SettingsSnapshot['ai']> = {}): SettingsSnapshot {
	return {
		ai: {
			provider: 'openai',
			model: 'gpt-4o-mini',
			customBaseUrl: '',
			apiKey: { set: true, source: 'settings' },
			...overrides,
		},
		bring: {
			email: '',
			emailSource: 'none',
			password: { set: false, source: 'none' },
		},
	};
}

describe('isAiConfigured', () => {
	it('reports an unloaded snapshot as not configured', () => {
		expect(isAiConfigured(null)).toBe(false);
	});

	it('requires a provider and a model', () => {
		expect(isAiConfigured(snapshot({ provider: '', model: 'gpt-4o-mini' }))).toBe(false);
		expect(isAiConfigured(snapshot({ model: '' }))).toBe(false);
	});

	it('requires a key for providers that need one', () => {
		expect(isAiConfigured(snapshot({ apiKey: { set: false, source: 'none' } }))).toBe(false);
		expect(isAiConfigured(snapshot({ apiKey: { set: true, source: 'environment' } }))).toBe(true);
	});

	it('accepts ollama without a key', () => {
		expect(
			isAiConfigured(snapshot({ provider: 'ollama', apiKey: { set: false, source: 'none' } })),
		).toBe(true);
	});

	it('requires a base URL for the custom provider', () => {
		const base = { provider: 'custom', apiKey: { set: false, source: 'none' as const } };
		expect(isAiConfigured(snapshot({ ...base, customBaseUrl: '' }))).toBe(false);
		expect(isAiConfigured(snapshot({ ...base, customBaseUrl: 'http://localhost:8080/v1/' }))).toBe(true);
	});
});

describe('SettingsCommitter', () => {
	it('keeps commits in order so a slow response cannot overwrite a newer value', async () => {
		const resolvers: Array<(value: SettingsSnapshot) => void> = [];
		const send = vi.fn(
			() => new Promise<SettingsSnapshot>((resolve) => resolvers.push(resolve)),
		);
		const applied: string[] = [];
		const committer = new SettingsCommitter(send, (s) => applied.push(s.ai.model));

		const first = committer.commit({ ai: { model: 'a' } });
		const second = committer.commit({ ai: { model: 'b' } });
		expect(send).toHaveBeenCalledTimes(1);

		resolvers[0](snapshot({ model: 'a' }));
		await first;
		expect(send).toHaveBeenCalledTimes(2);

		resolvers[1](snapshot({ model: 'b' }));
		await second;

		expect(applied).toEqual(['a', 'b']);
		expect(committer.state.status).toBe('saved');
		expect(committer.state.error).toBeNull();
	});

	it('surfaces a failed commit and recovers on the next success', async () => {
		const send = vi
			.fn()
			.mockRejectedValueOnce(new Error('provider must be one of openai'))
			.mockResolvedValueOnce(snapshot());
		const committer = new SettingsCommitter(send, () => {});

		await committer.commit({ ai: { provider: 'nope' } });
		expect(committer.state.status).toBe('error');
		expect(committer.state.error).toContain('provider must be one of');

		await committer.commit({ ai: { provider: 'openai' } });
		expect(committer.state.status).toBe('saved');
		expect(committer.state.error).toBeNull();
	});

	it('reports a saving state while the request is in flight', async () => {
		let resolve!: (value: SettingsSnapshot) => void;
		const send = vi.fn(() => new Promise<SettingsSnapshot>((r) => (resolve = r)));
		const committer = new SettingsCommitter(send, () => {});

		const pending = committer.commit({ ai: { model: 'a' } });
		expect(committer.state.status).toBe('saving');

		resolve(snapshot());
		await pending;
		expect(committer.state.status).toBe('saved');
	});
});
