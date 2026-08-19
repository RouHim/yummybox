import { describe, it, expect } from 'vitest';
import { llmErrorMessage } from './llm-error';
import { ApiError } from './api';
import { t } from './i18n';

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

	it('passes through the raw message for llm_api_key_missing', () => {
		expect(llmErrorMessage(new ApiError('Set OPENAI_API_KEY', 'llm_api_key_missing', 400))).toBe(
			'Set OPENAI_API_KEY'
		);
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
