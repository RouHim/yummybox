import { test, expect, type Page } from '@playwright/test';
import { resetMeals, resetSettings, setLocale } from './_helpers';

// The custom OpenAI-compatible provider is aimed at a closed local port: the
// app never reaches a real LLM provider, and the model listing always fails.
const DEAD_ENDPOINT = 'http://127.0.0.1:1/v1/';

type BringStatus = { configured: boolean; connected: boolean; error: string | null };

/**
 * Answer the Bring! status probe the page (and the app-bar footer) runs. A
 * function is re-evaluated per request, so a mock can change its answer once a
 * commit has landed.
 */
async function mockBringStatus(page: Page, body: BringStatus | (() => BringStatus)): Promise<void> {
	await page.route('**/api/bring/status', async (route) => {
		const status = typeof body === 'function' ? body() : body;
		await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(status) });
	});
}

test.describe('Settings page', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
		await resetSettings(request);
		// Stored credentials never reach the real Bring! API: every test
		// answers the probe locally, and none of them inherits an ambient
		// BRING_EMAIL/BRING_PASSWORD.
		await mockBringStatus(page, { configured: false, connected: false, error: null });
	});

	test('given_app_bar_when_gear_clicked_then_settings_page_opens', async ({ page }) => {
		await page.goto('/meals');
		await page.getByRole('link', { name: 'Settings' }).click();
		await expect(page).toHaveURL(/\/settings$/);
		await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible();
	});

	test('given_fresh_database_when_provider_chosen_then_option_selectable_and_key_field_shown', async ({ page }) => {
		// The real provider list is the thing under test: a fresh database with
		// no provider environment variables reports every keyed provider as not
		// configured, and that must not make it unselectable - a provider whose
		// option is disabled can never have its key stored from the UI. Only the
		// model listing is mocked, so no request can leave for a real provider.
		await page.route('**/api/llm/models?*', async (route) => {
			await route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'API key not configured', code: 'llm_not_configured' }),
			});
		});

		await page.goto('/settings');
		const providerSelect = page.locator('select').first();
		// The real list loads asynchronously; wait for a known provider before
		// counting options, otherwise the empty pre-load select is measured.
		await expect(providerSelect.locator('option[value="openai"]')).toHaveCount(1);

		// Every provider in the real list must be choosable, whether or not it
		// already has a usable key.
		const options = providerSelect.locator('option[value]:not([value=""])');
		const optionCount = await options.count();
		expect(optionCount).toBeGreaterThan(0);
		for (let i = 0; i < optionCount; i++) {
			await expect(options.nth(i)).toBeEnabled();
		}

		// Choosing a keyed provider reveals the field its key is stored in.
		await providerSelect.selectOption('openai');
		await expect(page.getByLabel('API key')).toBeVisible();
	});

	test('given_provider_and_model_when_committed_then_saved_state_and_persisted', async ({ page }) => {
		await page.goto('/settings');

		await page.locator('select').first().selectOption('custom');
		await page.getByLabel('Base URL').fill(DEAD_ENDPOINT);
		await page.getByLabel('API Key (optional)').blur();
		await page.getByLabel('Base URL').blur();

		// The model listing against the dead endpoint fails, so the model
		// becomes a free-text field; committing it must still be reported.
		const modelInput = page.getByPlaceholder('Model name (e.g. gpt-4o-mini)');
		await expect(modelInput).toBeVisible();
		await modelInput.fill('test-model');
		await modelInput.blur();

		await expect(page.getByText('Saved')).toBeVisible();

		// Reload: the commit survives because it is stored server-side.
		await page.reload();
		await expect(page.locator('select').first()).toHaveValue('custom');
		await expect(page.getByLabel('Base URL')).toHaveValue(DEAD_ENDPOINT);
		// The stored model survives too: the dead endpoint makes the listing fail
		// again, so the restored model is shown in the free-text field. Without a
		// listing on restore the model control would only offer its placeholder.
		await expect(page.getByPlaceholder('Model name (e.g. gpt-4o-mini)')).toHaveValue('test-model');
	});

	test('given_stored_api_key_when_page_loads_then_state_shown_and_value_never_rendered', async ({ page, request }) => {
		const patch = await request.patch('/api/settings', {
			data: {
				ai: {
					provider: 'custom',
					model: 'test-model',
					customBaseUrl: DEAD_ENDPOINT,
					apiKey: 'super-secret-key',
				},
			},
		});
		expect(patch.ok()).toBe(true);

		await page.goto('/settings');

		// Scoped to the AI card's own state line: the Bring! card renders its
		// own "Not set" labels, so an unscoped locator would stay green even
		// if the clear commit never reached the server.
		const aiSecretState = page.locator('.settings-card').first().locator('.llm-secret-state');
		await expect(aiSecretState).toContainText('Stored in settings');
		await expect(page.locator('body')).not.toContainText('super-secret-key');

		const res = await request.get('/api/settings');
		expect(await res.text()).not.toContain('super-secret-key');

		// Clearing falls back to "not set" when no environment value exists.
		await page.getByRole('button', { name: 'Clear' }).first().click();
		await expect(aiSecretState).toHaveText('Not set');
	});

	test('given_stored_bring_password_when_page_loads_then_value_never_rendered', async ({ page, request }) => {
		await request.patch('/api/settings', {
			data: { bring: { email: 'cook@example.com', password: 'bring-secret-pass' } },
		});

		await page.goto('/settings');

		await expect(page.getByLabel('Bring! email')).toHaveValue('cook@example.com');
		await expect(page.getByText('Stored in settings').first()).toBeVisible();
		await expect(page.locator('body')).not.toContainText('bring-secret-pass');
		// The body text cannot see an input's value, so pin the field itself.
		await expect(page.getByLabel('Bring! password')).toHaveValue('');

		const res = await request.get('/api/settings');
		expect(await res.text()).not.toContain('bring-secret-pass');
	});

	test('given_rejected_connection_when_credentials_committed_then_error_shown_inline', async ({ page }) => {
		// The failing answer only arrives once a commit landed: the page probes
		// the endpoint on mount too, so a static mock would let this assertion
		// pass without the post-commit re-probe (FR-011) existing at all.
		let credentialsCommitted = false;
		await page.route('**/api/settings', async (route) => {
			const isCommit = route.request().method() === 'PATCH';
			const response = await route.fetch();
			if (isCommit) credentialsCommitted = true;
			await route.fulfill({ response });
		});
		// Registered after the default probe mock, so it takes precedence.
		await mockBringStatus(page, () =>
			credentialsCommitted
				? {
						configured: true,
						connected: false,
						error: 'Bring! login failed, check your Bring! credentials in Settings',
					}
				: { configured: true, connected: false, error: null }
		);

		await page.goto('/settings');
		// The app-bar layout probes the same endpoint for its footer, which
		// repeats the message, so scope this to the page itself.
		const inlineError = page.getByRole('main').getByText('Bring! login failed');
		await expect(inlineError).toBeHidden();

		const email = page.getByLabel('Bring! email');
		await email.fill('cook@example.com');
		await email.blur();
		await expect(inlineError).toBeVisible();

		const password = page.getByLabel('Bring! password');
		await password.fill('wrong-password');
		await password.blur();

		await expect(inlineError).toBeVisible();
		await expect(page.getByLabel('Bring! password')).toHaveValue('');
	});

	test('given_no_stored_password_when_settings_load_then_password_field_asks_for_a_new_one', async ({ page }) => {
		// Without a stored or inherited password there is nothing to replace, so
		// the field must not claim there is.
		await page.goto('/settings');

		await expect(page.getByLabel('Bring! password')).toHaveAttribute('placeholder', 'Enter your Bring! password');
	});

	test('given_no_ai_configuration_when_generate_page_opened_then_notice_links_to_settings', async ({ page }) => {
		await page.goto('/spontaneous');
		const notice = page.locator('.ai-config-notice');
		await expect(notice).toBeVisible();
		await notice.getByRole('link', { name: 'Open settings' }).click();
		await expect(page).toHaveURL(/\/settings$/);
	});
});
