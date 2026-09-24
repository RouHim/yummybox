/// <reference types="svelte" />

import type { TranslationKey } from './i18n/types';

export type ValueSource = 'settings' | 'environment' | 'none';

/** A secret as the API reports it: never the value itself. */
export interface SecretState {
	set: boolean;
	source: ValueSource;
}

export interface AiSettings {
	provider: string;
	model: string;
	customBaseUrl: string;
	apiKey: SecretState;
}

export interface BringSettings {
	email: string;
	emailSource: ValueSource;
	password: SecretState;
}

export interface SettingsSnapshot {
	ai: AiSettings;
	bring: BringSettings;
}

/** `null` clears a stored value, a string replaces it, an absent key leaves it untouched. */
export interface AiPatch {
	provider?: string | null;
	model?: string | null;
	customBaseUrl?: string | null;
	apiKey?: string | null;
}

export interface BringPatch {
	email?: string | null;
	password?: string | null;
}

export interface SettingsPatch {
	ai?: AiPatch;
	bring?: BringPatch;
}

/** Whether every AI flow has the configuration it needs to run. */
export function isAiConfigured(settings: SettingsSnapshot | null): boolean {
	if (!settings) return false;
	const { provider, model, customBaseUrl, apiKey } = settings.ai;
	if (!provider || !model) return false;
	// `custom` is usable as soon as its endpoint is known, `ollama` without a
	// key at all, mirroring `llm_import::needs_no_api_key` server-side.
	if (provider === 'custom') return customBaseUrl.trim().length > 0;
	if (provider === 'ollama') return true;
	return apiKey.set;
}

/** i18n key describing where a value comes from. */
export function secretSourceLabelKey(source: ValueSource): TranslationKey {
	if (source === 'settings') return 'settingsSecretStored';
	if (source === 'environment') return 'settingsSecretInherited';
	return 'settingsSecretAbsent';
}

export type CommitStatus = 'idle' | 'saving' | 'saved' | 'error';

/** i18n key describing a commit state, or `null` while idle. */
export function commitStatusLabelKey(status: CommitStatus): TranslationKey | null {
	if (status === 'saving') return 'settingsSaving';
	if (status === 'saved') return 'settingsSaved';
	if (status === 'error') return 'settingsSaveFailed';
	return null;
}

/** Chip modifier carrying the same commit state, so every surface reads alike. */
export function commitStatusChipClass(status: CommitStatus): string {
	if (status === 'error') return 'state-chip--alert';
	if (status === 'saved') return 'state-chip--ok';
	return 'state-chip--neutral';
}

/**
 * Whether a provider selection really moves off the provider the server last
 * confirmed. While that provider is unknown (`null`: the mount-time read has not
 * answered yet, or failed) the selection changed nothing definite, so neither
 * the form nor the commit may delete what may still be this provider's model,
 * endpoint and key.
 */
export function providerChanged(selected: string, storedProvider: string | null): boolean {
	return storedProvider !== null && storedProvider !== selected;
}

/**
 * The patch a provider selection sends: only a genuine switch may delete the
 * previous provider's dependent values, so the server keeps them for a
 * selection that changed nothing.
 */
export function providerChangePatch(selected: string, storedProvider: string | null): AiPatch {
	if (!providerChanged(selected, storedProvider)) return { provider: selected };
	return { provider: selected, model: null, customBaseUrl: null, apiKey: null };
}

/**
 * Whether a typed secret may be dropped from the DOM after its commit: only a
 * stored value is safe to forget. A commit that failed resolves like a stored
 * one, so the value must stay in the field for a retry.
 */
export function commitStored(status: CommitStatus): boolean {
	return status === 'saved';
}

/**
 * Whether the "no AI provider configured" notice belongs on screen: only after
 * a successful read reported nothing usable. A read that is still pending or
 * that failed leaves the answer unknown, and an install whose settings hold a
 * complete configuration must never be reported as unconfigured.
 */
export function aiConfigNoticeVisible(loaded: boolean, configured: boolean): boolean {
	return loaded && !configured;
}

/**
 * Whether an AI flow may be attempted: only a successful read that found no
 * configuration blocks it. While the stored configuration is still unknown the
 * server stays authoritative and answers a premature request with its own
 * `llm_not_configured` error.
 */
export function aiFlowReady(loaded: boolean, configured: boolean): boolean {
	return !loaded || configured;
}

/**
 * Serializes settings commits so a slow earlier request can never land after a
 * newer one and overwrite it, and exposes the shared saving/saved/failed state
 * that every commit must show.
 */
export class SettingsCommitter {
	state = $state<{ status: CommitStatus; error: string | null }>({
		status: 'idle',
		error: null,
	});
	/** Commits accepted but not answered yet; the chain runs one at a time. */
	#pending = 0;
	#tail: Promise<void> = Promise.resolve();
	#send: (patch: SettingsPatch) => Promise<SettingsSnapshot>;
	#apply: (snapshot: SettingsSnapshot) => void;

	constructor(
		send: (patch: SettingsPatch) => Promise<SettingsSnapshot>,
		apply: (snapshot: SettingsSnapshot) => void,
	) {
		this.#send = send;
		this.#apply = apply;
	}

	commit(patch: SettingsPatch): Promise<void> {
		const run = async () => {
			this.state = { status: 'saving', error: null };
			try {
				const snapshot = await this.#send(patch);
				this.#apply(snapshot);
				this.state = { status: 'saved', error: null };
			} catch (err) {
				this.state = {
					status: 'error',
					error: err instanceof Error ? err.message : String(err),
				};
			} finally {
				this.#pending--;
			}
		};
		// An idle committer sends at once; a busy one queues behind the request
		// in flight so an older answer can never overwrite a newer value.
		this.#pending++;
		const queued = this.#pending === 1 ? run() : this.#tail.then(run);
		this.#tail = queued;
		return queued;
	}
}
