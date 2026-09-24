import { describe, it, expect } from 'vitest';
import { llmErrorMessage } from './llm-error';
import { ApiError } from './api';
import { dictionaries, setLocale, t } from './i18n';

describe('llmErrorMessage', () => {
	it('maps REQUEST_FAILED to the fetch error message', () => {
		expect(llmErrorMessage(new ApiError('boom', 'REQUEST_FAILED', 500))).toBe(t('importErrorFetch'));
	});

	it('maps llm_timeout to the timeout message', () => {
		expect(llmErrorMessage(new ApiError('boom', 'llm_timeout', 500))).toBe(t('llmErrorTimeout'));
	});

	it('maps llm_parse_failed to the parse-failed message', () => {
		expect(llmErrorMessage(new ApiError('boom', 'llm_parse_failed', 500))).toBe(t('llmErrorParseFailed'));
	});

	it('maps llm_api_key_missing to the localized hint, not the raw server sentence', () => {
		const message = llmErrorMessage(
			new ApiError(
				"API key not configured for provider 'openai': store one in Settings or set the OPENAI_API_KEY environment variable",
				'llm_api_key_missing',
				400
			)
		);
		expect(message).toBe(t('llmErrorApiKey'));
		expect(message).not.toContain('OPENAI_API_KEY');
	});

	it('renders the German llm_api_key_missing hint, not the raw server sentence', () => {
		setLocale('de');
		try {
			const message = llmErrorMessage(
				new ApiError(
					"API key not configured for provider 'openai': store one in Settings or set the OPENAI_API_KEY environment variable",
					'llm_api_key_missing',
					400
				)
			);
			expect(message).toBe(dictionaries.de.llmErrorApiKey);
			expect(message).not.toContain('OPENAI_API_KEY');
		} finally {
			setLocale('en');
		}
	});

	it('maps llm_not_configured to the settings hint', () => {
		expect(llmErrorMessage(new ApiError('llm not configured', 'llm_not_configured', 400))).toBe(
			t('llmErrorNotConfigured')
		);
	});

	it('renders the German llm_not_configured hint, not the raw server sentence', () => {
		setLocale('de');
		try {
			const message = llmErrorMessage(
				new ApiError(
					'AI is not configured: choose a provider and a model in Settings',
					'llm_not_configured',
					400
				)
			);
			expect(message).toBe(dictionaries.de.llmErrorNotConfigured);
			expect(message).not.toContain('choose a provider');
		} finally {
			setLocale('en');
		}
	});

	it('wraps unknown codes in the generic message', () => {
		expect(llmErrorMessage(new ApiError('weird', 'other_code', 500))).toBe(
			t('llmErrorGeneric', { message: 'weird' })
		);
	});

	it('returns the raw message when code is null', () => {
		expect(llmErrorMessage(new ApiError('no code', null, 500))).toBe('no code');
	});

	it('maps TypeError to the fetch error message', () => {
		expect(llmErrorMessage(new TypeError('network down'))).toBe(t('importErrorFetch'));
	});

	it('returns the message for a plain Error', () => {
		expect(llmErrorMessage(new Error('plain failure'))).toBe('plain failure');
	});

	it('returns an empty string for non-error values', () => {
		expect(llmErrorMessage('just a string')).toBe('');
		expect(llmErrorMessage(undefined)).toBe('');
	});
});
