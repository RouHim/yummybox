import { describe, it, expect, vi } from 'vitest';
import {
	SettingsCommitter,
	aiConfigNoticeVisible,
	aiFlowReady,
	commitStored,
	isAiConfigured,
	providerChangePatch,
	providerChanged,
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

	it('resolves each queued commit with its own outcome, not the state a successor re-enters', async () => {
		// The picker's post-commit callbacks branch on the outcome of their own
		// write. A successor queued behind one sets the shared state back to
		// `saving` before the earlier callback runs, so an outcome read off the
		// state would report the stored write as rejected. The resolved value
		// has to be the commit's own answer.
		const resolvers: Array<(value: SettingsSnapshot) => void> = [];
		const send = vi.fn(() => new Promise<SettingsSnapshot>((resolve) => resolvers.push(resolve)));
		const committer = new SettingsCommitter(send, () => {});

		const storedWrite = committer.commit({ ai: { provider: 'openai' } });
		const successor = committer.commit({ ai: { provider: null } });

		resolvers[0](snapshot());
		expect(await storedWrite).toBe('saved');
		// The successor is in flight, so the shared state has already moved on:
		// this is exactly the value that must not stand in for the outcome.
		expect(committer.state.status).toBe('saving');

		resolvers[1](snapshot({ provider: '' }));
		expect(await successor).toBe('saved');
	});

	it('settle waits for every queued commit and never rejects on a failure', async () => {
		// The AI flows await this before sending a request that depends on the
		// configuration a click just stored: a settle that resolved early (or
		// rejected on a failed commit) would send the request against the
		// pre-commit state, or skip it entirely.
		const resolvers: Array<(value: SettingsSnapshot) => void> = [];
		const send = vi.fn(
			() => new Promise<SettingsSnapshot>((resolve) => resolvers.push(resolve)),
		);
		const committer = new SettingsCommitter(send, () => {});

		// Nothing queued: settle resolves at once.
		await committer.settle();

		const first = committer.commit({ ai: { model: 'a' } });
		committer.commit({ ai: { model: 'b' } });
		let settled = false;
		const waited = committer.settle().then(() => {
			settled = true;
		});
		await Promise.resolve();
		expect(settled).toBe(false);

		resolvers[0](snapshot({ model: 'a' }));
		await first;
		expect(settled).toBe(false);

		resolvers[1](snapshot({ model: 'b' }));
		await waited;
		expect(settled).toBe(true);
		expect(committer.state.status).toBe('saved');

		// A rejected commit is an answer like any other: settle resolves, so the
		// caller's request still runs and the server reports the real problem.
		send.mockRejectedValueOnce(new Error('boom'));
		committer.commit({ ai: { model: 'c' } });
		await expect(committer.settle()).resolves.toBeUndefined();
		expect(committer.state.status).toBe('error');
	});
});

describe('commitStored', () => {
	it('recognises only a stored commit as safe to forget the typed value', () => {
		expect(commitStored('idle')).toBe(false);
		expect(commitStored('saving')).toBe(false);
		expect(commitStored('error')).toBe(false);
		expect(commitStored('saved')).toBe(true);
	});

	it('keeps the typed value in the field when the commit is rejected', async () => {
		// A rejected commit resolves too (it never rejects), and its resolved
		// value is `error`: the DOM cleanup has to ask that outcome, or the
		// pasted key is destroyed and has to be fetched from the provider again.
		const send = vi.fn().mockRejectedValue(new Error('apiKey must be at most 4096 characters'));
		const committer = new SettingsCommitter(send, () => {});
		let field = 'sk-typed';

		const outcome = await committer.commit({ ai: { apiKey: field } });
		if (commitStored(outcome)) field = '';

		expect(outcome).toBe('error');
		expect(committer.state.status).toBe('error');
		expect(field).toBe('sk-typed');
	});

	it('keeps the typed Bring! password when its commit is rejected', async () => {
		// Same rule for the settings page's password field: an over-long or
		// otherwise rejected password must stay typed in for a retry.
		const send = vi.fn().mockRejectedValue(new Error('password must be at most 256 characters'));
		const committer = new SettingsCommitter(send, () => {});
		let field = 'typed-pass';

		const outcome = await committer.commit({ bring: { password: field } });
		if (commitStored(outcome)) field = '';

		expect(outcome).toBe('error');
		expect(committer.state.status).toBe('error');
		expect(field).toBe('typed-pass');
	});

	it('clears the typed value once the commit is stored', async () => {
		const send = vi.fn().mockResolvedValue(snapshot());
		const committer = new SettingsCommitter(send, () => {});
		let field = 'sk-typed';

		const outcome = await committer.commit({ ai: { apiKey: field } });
		if (commitStored(outcome)) field = '';

		expect(outcome).toBe('saved');
		expect(committer.state.status).toBe('saved');
		expect(field).toBe('');
	});
});

describe('provider selection', () => {
	it('keeps the stored model, endpoint and key when the selection did not move', () => {
		// A user on a slow link re-picks the stored provider before the
		// mount-time read has answered. Nothing may be deleted: the values the
		// read would have shown are the ones this commit would destroy.
		expect(providerChangePatch('openai', 'openai')).toEqual({ provider: 'openai' });
		expect(providerChanged('openai', 'openai')).toBe(false);
	});

	it('keeps them while the stored provider has not been read', () => {
		// Unknown stored provider: the selection may well be the stored one, so
		// only the provider itself is committed.
		expect(providerChangePatch('openai', null)).toEqual({ provider: 'openai' });
		expect(providerChanged('openai', null)).toBe(false);
	});

	it('clears them when the selection really moves off the stored provider', () => {
		expect(providerChangePatch('anthropic', 'openai')).toEqual({
			provider: 'anthropic',
			model: null,
			customBaseUrl: null,
			apiKey: null,
		});
		expect(providerChanged('anthropic', 'openai')).toBe(true);
	});

	it('clears them when the provider is removed altogether', () => {
		expect(providerChangePatch('', 'openai')).toEqual({
			provider: '',
			model: null,
			customBaseUrl: null,
			apiKey: null,
		});
		expect(providerChanged('', 'openai')).toBe(true);
	});
});

describe('unknown stored configuration', () => {
	it('never reports an unread configuration as unconfigured', () => {
		// A pending read and a failed read both leave `loaded` false: neither may
		// render the "no provider configured" notice, and neither may block a
		// flow the server is still able to answer.
		expect(aiConfigNoticeVisible(false, false)).toBe(false);
		expect(aiFlowReady(false, false)).toBe(true);
	});

	it('reports the notice and blocks the flow only once a read found no configuration', () => {
		expect(aiConfigNoticeVisible(true, false)).toBe(true);
		expect(aiFlowReady(true, false)).toBe(false);
		expect(aiConfigNoticeVisible(true, true)).toBe(false);
		expect(aiFlowReady(true, true)).toBe(true);
	});
});
