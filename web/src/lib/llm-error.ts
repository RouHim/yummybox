import { ApiError } from '$lib/api';
import { t } from '$lib/i18n';

/**
 * Maps LLM-import/generate API errors to user-facing messages.
 * Shared by the AI-import flow (add-meal dialog) and the Generate page.
 */
export function llmErrorMessage(err: unknown): string {
	if (err instanceof ApiError) {
		if (err.code === 'llm_timeout') return t('llmErrorTimeout');
		if (err.code === 'llm_parse_failed') return t('llmErrorParseFailed');
		if (err.code) return t('llmErrorGeneric', { message: err.message });
		return err.code === 'REQUEST_FAILED' ? t('importErrorFetch') : err.message;
	}
	return err instanceof Error ? err.message : '';
}
