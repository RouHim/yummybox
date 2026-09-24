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

/**
 * Hold every `GET /api/settings` answer until `release` is called, so a read
 * that started before the user acted is delivered after it. The body is
 * fetched up front, which makes the late answer stale by construction.
 * `settled` resolves once every held read has been answered, so a caller can
 * wait for the stale answers to have been delivered before judging state.
 */
async function holdSettingsReads(
	page: Page
): Promise<{ release: () => void; settled: Promise<void> }> {
	const gate = Promise.withResolvers<void>();
	const settled = Promise.withResolvers<void>();
	let held = 0;
	let answered = 0;
	await page.route('**/api/settings*', async (route) => {
		if (route.request().method() !== 'GET') {
			await route.continue();
			return;
		}
		held++;
		const response = await route.fetch();
		await gate.promise;
		await route.fulfill({ response });
		answered++;
		if (answered === held) settled.resolve();
	});
	return { release: gate.resolve, settled: settled.promise };
}

test.describe('Settings page', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
		await resetSettings(request);
		// Stored credentials never reach the real Bring! API: every test
		// answers the probe locally. The server does inherit this shell's
		// BRING_EMAIL/BRING_PASSWORD (Playwright merges process.env into the
		// webServer environment), so no test may assume those variables are
		// unset; assertions have to derive the expected state from the API.
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

	test('given_rejected_commit_when_a_secret_committed_then_error_shown_and_typed_value_kept', async ({ page, request }) => {
		// Every other test drives commits that succeed. A rejected commit must
		// say so and must keep the typed secret in its field: a pasted API key is
		// often not reproducible, and a field cleared without storing the value
		// would claim a state the server does not have.
		await page.route('**/api/settings', async (route) => {
			if (route.request().method() !== 'PATCH') {
				await route.continue();
				return;
			}
			await route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({
					error: 'apiKey must be at most 4096 characters',
					code: 'validation',
				}),
			});
		});

		// Selecting a provider fires the model listing regardless of the commit
		// outcome, so it is mocked here too: on a shell exporting OPENAI_API_KEY
		// an unmocked listing would leave for the real provider.
		await page.route('**/api/llm/models?*', (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['gpt-4o-mini'] }),
			})
		);

		await page.goto('/settings');
		const providerSelect = page.locator('select').first();
		await expect(providerSelect.locator('option[value="openai"]')).toHaveCount(1);
		await providerSelect.selectOption('openai');

		const apiKey = page.getByLabel('API key');
		await apiKey.fill('sk-typed-key');
		await apiKey.blur();

		await expect(page.locator('.llm-commit-state')).toHaveText('Could not save');
		// The server's own sentence is rendered below that chip: a regression
		// that stops showing it would leave the user with a failure they cannot
		// act on, and the suite would stay green without this assertion.
		await expect(page.locator('.llm-fields .form-error')).toHaveText(
			'apiKey must be at most 4096 characters'
		);
		await expect(apiKey).toHaveValue('sk-typed-key');

		const password = page.getByLabel('Bring! password');
		await password.fill('typed-pass');
		await password.blur();

		await expect(page.locator('.settings-card').nth(1).locator('.settings-commit')).toHaveText(
			'Could not save'
		);
		await expect(
			page.locator('.settings-card').nth(1).locator('.form-error')
		).toHaveText('apiKey must be at most 4096 characters');
		await expect(password).toHaveValue('typed-pass');

		// Both commits were rejected, so neither value reached the server.
		const res = await request.get('/api/settings');
		const stored = (await res.json()) as {
			ai: { provider: string; apiKey: { set: boolean } };
			bring: { password: { set: boolean; source: string } };
		};
		expect(stored.ai.provider).toBe('');
		expect(stored.ai.apiKey.set).toBe(false);
		// `set` alone cannot pin the rejected commit: a shell that exports
		// BRING_PASSWORD reaches the server through Playwright's environment
		// merge, so the reported value has to stay un-stored, whatever the
		// environment supplies.
		expect(stored.bring.password.source).not.toBe('settings');
	});

	test('given_a_key_committed_before_its_endpoint_then_the_real_rejection_names_the_endpoint', async ({ page, request }) => {
		// The real key-without-endpoint rejection, in the UI order that reaches
		// it and with no mock in the way: selecting `custom` and pasting a key
		// commits the key before any endpoint exists, so the API refuses it and
		// names the field that is missing. The faked rejection above cannot pin
		// the sentence the server actually sends, and this is the only test that
		// drives it end to end.
		await page.goto('/settings');
		const providerSelect = page.locator('select').first();
		await expect(providerSelect.locator('option[value="custom"]')).toHaveCount(1);
		await providerSelect.selectOption('custom');

		const apiKey = page.getByLabel('API Key (optional)');
		await apiKey.fill('sk-typed-key');
		await apiKey.blur();

		await expect(page.locator('.llm-commit-state')).toHaveText('Could not save');
		await expect(page.locator('.llm-fields .form-error')).toHaveText(
			'customBaseUrl must be set before an apiKey is stored for the custom provider'
		);
		// A rejected commit stores nothing and keeps the typed secret for the
		// resend its message asks for.
		await expect(apiKey).toHaveValue('sk-typed-key');
		const before = await request.get('/api/settings');
		const beforeSnapshot = (await before.json()) as { ai: { apiKey: { source: string } } };
		expect(beforeSnapshot.ai.apiKey.source).not.toBe('settings');

		// With the endpoint stored, the same key has somewhere to bind: the
		// commit that follows is accepted, and the typed key is still waiting in
		// its field for the resend.
		const baseUrl = page.getByLabel('Base URL');
		await baseUrl.fill(DEAD_ENDPOINT);
		await baseUrl.blur();
		await expect(page.locator('.llm-commit-state')).toHaveText('Saved');
		await expect(apiKey).toHaveValue('sk-typed-key');
	});

	test('given_settings_read_answered_late_when_provider_chosen_then_choice_survives', async ({ page }) => {
		// A user on a slow link picks a provider and stores its key while the
		// page is still reading the stored configuration. That read captured a
		// state without the key, so its late answer is stale in content: applying
		// it would undo the choice and its key, and the assertions below fail if
		// the sequence guard is deleted.
		const { release, settled } = await holdSettingsReads(page);

		await page.goto('/settings');
		const providerSelect = page.locator('select').first();
		await expect(providerSelect.locator('option[value="custom"]')).toHaveCount(1);
		await providerSelect.selectOption('custom');
		await expect(page.getByLabel('Base URL')).toBeVisible();

		// The key is bound to the endpoint it is stored for, so the endpoint is
		// committed first and the picker serializes the two commits: a key sent
		// before any endpoint exists is rejected, which is not what this test is
		// about.
		await page.getByLabel('Base URL').fill(DEAD_ENDPOINT);
		await page.getByLabel('Base URL').blur();
		const apiKey = page.getByLabel('API Key (optional)');
		await apiKey.fill('sk-slow-link');
		await apiKey.blur();
		const aiSecretState = page.locator('.settings-card').first().locator('.llm-secret-state');
		await expect(aiSecretState).toHaveText('Stored in settings');

		// The page's own Bring! read is held by the same gate, so a credential
		// committed now is loaded by the commit answer while that read is still
		// in flight. Its late, stale answer would undo the commit: the assertion
		// after the release fails if the page's own sequence guard is deleted.
		const bringEmail = page.getByLabel('Bring! email');
		const bringEmailChip = page
			.locator('.settings-card')
			.nth(1)
			.locator('.field')
			.filter({ has: page.getByLabel('Bring! email') })
			.locator('.state-chip');
		await bringEmail.fill('cook@example.com');
		await bringEmail.blur();
		await expect(bringEmailChip).toHaveText('Stored in settings');

		// Both held reads share the gate: wait for their stale answers to have
		// been delivered before judging, so the guards get their chance to drop
		// them.
		release();
		await settled;

		await expect(providerSelect).toHaveValue('custom');
		await expect(page.getByLabel('Base URL')).toBeVisible();
		await expect(aiSecretState).toHaveText('Stored in settings');
		await expect(bringEmailChip).toHaveText('Stored in settings');
	});

	test('given_a_slow_read_landing_between_a_provider_switch_and_its_commit_then_the_previous_model_is_not_shown', async ({ page, request }) => {
		// A user on a slow link switches provider before the mount read answers.
		// The read captured the previous provider's state, so its late answer
		// must not fill the model and the endpoint it still reports: the server
		// deleted both with the switch, and a model the server does not have
		// makes the listing report a network/key error and every AI flow answer
		// `llm_not_configured` although a model is on screen.
		const stored = await request.patch('/api/settings', {
			data: { ai: { provider: 'openai', model: 'gpt-4o-mini' } },
		});
		expect(stored.ok()).toBe(true);
		// The listing succeeds for the new provider and does not offer the old
		// model, so the model control shows exactly what the picker believes.
		await page.route('**/api/llm/models?*', (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['claude-3-5-haiku'] }),
			})
		);

		// Both the mount read and the switch commit are held; the read is
		// released first, so its stale answer lands between the switch and its
		// own answer.
		const readGate = Promise.withResolvers<void>();
		const commitGate = Promise.withResolvers<void>();
		let heldReads = 0;
		const commitHeld = Promise.withResolvers<void>();
		await page.route('**/api/settings*', async (route) => {
			// Fetching up front makes the held answer stale by construction: it
			// carries the state as it was when the request was issued.
			const response = await route.fetch();
			if (route.request().method() === 'GET') {
				heldReads++;
				await readGate.promise;
			} else {
				commitHeld.resolve();
				await commitGate.promise;
			}
			await route.fulfill({ response });
		});

		await page.goto('/settings');
		// The picker's read and the page's own read are both on their way; the
		// picker's answer is the one that fills the AI fields.
		await expect.poll(() => heldReads).toBe(2);
		const providerSelect = page.locator('select').first();
		await expect(providerSelect.locator('option[value="anthropic"]')).toHaveCount(1);
		await providerSelect.selectOption('anthropic');
		await commitHeld.promise;

		// The stale read is delivered first. The key chip only renders once a
		// snapshot of this mount has been applied, so its appearance proves the
		// read was applied before the commit answer below.
		readGate.resolve();
		await expect(page.locator('.llm-secret-state')).toHaveCount(1);
		commitGate.resolve();

		// The switch deleted the OpenAI model, so the picker must not show it:
		// the listing lands (its model is an option, so the model control is the
		// select and not the free-text fallback a failed listing shows) and the
		// old provider's model is neither offered nor selected.
		await expect(providerSelect).toHaveValue('anthropic');
		await expect(page.locator('#llm-model option[value="claude-3-5-haiku"]')).toHaveCount(1);
		await expect(page.locator('#llm-model')).toHaveValue('');
		await expect(page.locator('#llm-model option[value="gpt-4o-mini"]')).toHaveCount(0);

		const res = await request.get('/api/settings');
		const snapshot = (await res.json()) as { ai: { provider: string; model: string } };
		expect(snapshot.ai.provider).toBe('anthropic');
		expect(snapshot.ai.model).toBe('');
	});

	test('given_a_rejected_base_url_when_committed_then_no_listing_runs', async ({ page }) => {
		// The commit never rejects its promise, so without a status guard the
		// base URL path would list the stored endpoint's models while the field
		// shows the rejected URL: a model chosen from that list would be
		// committed for the stored endpoint instead.
		let listings = 0;
		await page.route('**/api/llm/models?*', async (route) => {
			listings++;
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['local-model'] }),
			});
		});
		// Only the base URL commit is rejected: the provider selection that
		// precedes it must succeed, otherwise the rejection below could be the
		// earlier one and the count would say nothing. A genuine provider switch
		// sends `customBaseUrl: null` along with it, so the match is narrowed to
		// the string value the base URL commit carries: matching the field name
		// would reject that switch too.
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"customBaseUrl":"')) {
				await route.continue();
				return;
			}
			await route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({
					error: 'customBaseUrl must start with http:// or https://',
					code: 'validation',
				}),
			});
		});

		await page.goto('/settings');
		const providerSelect = page.locator('select').first();
		await expect(providerSelect.locator('option[value="custom"]')).toHaveCount(1);
		await providerSelect.selectOption('custom');
		// The provider commit is answered before the base URL is typed, so the
		// rejection below can only be the base URL's, and the listing that
		// follows a provider selection runs on an empty endpoint (the stored
		// one is not read yet) instead of on the URL being typed.
		await expect(page.locator('.llm-commit-state')).toHaveText('Saved');

		const baseUrl = page.getByLabel('Base URL');
		await expect(baseUrl).toBeVisible();
		await baseUrl.fill('localhost:1/v1/');
		await baseUrl.blur();
		// The rejection is reported, which proves this commit was answered ...
		await expect(page.locator('.llm-commit-state')).toHaveText('Could not save');
		// ... and the field keeps the URL the server refused.
		await expect(baseUrl).toHaveValue('localhost:1/v1/');
		// Nothing was stored, so the stored endpoint's models must not be
		// listed: a window for a stray request to land before the count is
		// judged.
		await page.waitForTimeout(300);
		expect(listings).toBe(0);
	});

	test('given_a_late_read_landing_after_a_bring_email_clear_then_the_cleared_field_stays_cleared', async ({ page, request }) => {
		// The field shows nothing while the mount read hangs, so the user
		// commits the empty value (a clear) before the read answers. That read
		// captured the value the clear removed, so refilling the empty field
		// from it would show an email the server no longer has while the chip
		// reports the cleared state.
		//
		// The shell may export BRING_EMAIL, so the value a clear falls back to
		// is read from the API instead of assumed; the stored value is made
		// distinct from it so the stale read cannot be mistaken for the
		// fallback.
		const before = await request.get('/api/settings');
		const beforeSnapshot = (await before.json()) as { bring: { email: string } };
		const beforeEmail = beforeSnapshot.bring.email;
		const storedEmail =
			beforeEmail === 'stale-read@example.com'
				? 'other-stale-read@example.com'
				: 'stale-read@example.com';
		const stored = await request.patch('/api/settings', {
			data: { bring: { email: storedEmail } },
		});
		expect(stored.ok()).toBe(true);

		const readGate = Promise.withResolvers<void>();
		const commitGate = Promise.withResolvers<void>();
		let heldReads = 0;
		const commitHeld = Promise.withResolvers<void>();
		await page.route('**/api/settings*', async (route) => {
			const response = await route.fetch();
			if (route.request().method() === 'GET') {
				heldReads++;
				await readGate.promise;
			} else {
				commitHeld.resolve();
				await commitGate.promise;
			}
			await route.fulfill({ response });
		});

		await page.goto('/settings');
		await expect.poll(() => heldReads).toBe(2);
		const email = page.getByLabel('Bring! email');
		await expect(email).toBeVisible();

		// Enter commits the empty field as a clear without needing it to lose
		// focus first.
		await email.press('Enter');
		await commitHeld.promise;

		// The stale read is delivered first: the provenance chip only renders
		// once a snapshot has been applied, so its appearance proves the read
		// landed before the commit answer below.
		readGate.resolve();
		const emailChip = page
			.locator('.settings-card')
			.nth(1)
			.locator('.field')
			.filter({ has: page.getByLabel('Bring! email') })
			.locator('.state-chip');
		await expect(emailChip).toHaveCount(1);
		commitGate.resolve();

		// The clear stands: the field shows what the server now reports, never
		// the value the stale read still carried.
		const after = await request.get('/api/settings');
		const afterSnapshot = (await after.json()) as { bring: { email: string } };
		const afterEmail = afterSnapshot.bring.email;
		expect(afterEmail).not.toBe(storedEmail);
		await expect(emailChip).not.toHaveText('Stored in settings');
		await expect(email).toHaveValue(afterEmail);
	});

	test('given_settings_read_answered_late_when_the_stored_provider_chosen_then_its_config_survives', async ({ page, request }) => {
		// A user on a slow link picks the provider that turns out to be the stored
		// one while the page is still reading it. A selection that did not move
		// must not delete the model it would have shown, and the key the user
		// stores while the read hangs must survive the late answer too: that
		// answer captured a state without the key, so applying it would undo a
		// commit the server already accepted.
		const stored = await request.patch('/api/settings', {
			data: { ai: { provider: 'openai', model: 'gpt-4o-mini' } },
		});
		expect(stored.ok()).toBe(true);
		await page.route('**/api/llm/models?*', (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['gpt-4o-mini'] }),
			})
		);

		const { release, settled } = await holdSettingsReads(page);

		await page.goto('/settings');
		const providerSelect = page.locator('select').first();
		await expect(providerSelect.locator('option[value="openai"]')).toHaveCount(1);
		await providerSelect.selectOption('openai');
		await expect(page.locator('.llm-commit-state')).toHaveText('Saved');

		// The commit answer carries the stored state, so the picker shows the
		// model the selection would otherwise have deleted.
		await expect(page.locator('#llm-model')).toHaveValue('gpt-4o-mini');

		const apiKey = page.getByLabel('API key');
		await apiKey.fill('sk-slow-link');
		await apiKey.blur();
		const aiSecretState = page.locator('.settings-card').first().locator('.llm-secret-state');
		await expect(aiSecretState).toHaveText('Stored in settings');

		// The page's own Bring! read is held by the same gate, so a credential
		// committed now is loaded by the commit answer while that read is still
		// in flight. Its late, stale answer would undo the commit: the assertion
		// after the release fails if the page's own sequence guard is deleted.
		const bringEmail = page.getByLabel('Bring! email');
		const bringEmailChip = page
			.locator('.settings-card')
			.nth(1)
			.locator('.field')
			.filter({ has: page.getByLabel('Bring! email') })
			.locator('.state-chip');
		await bringEmail.fill('cook@example.com');
		await bringEmail.blur();
		await expect(bringEmailChip).toHaveText('Stored in settings');

		// Both held reads share the gate: wait for their stale answers to have
		// been delivered before judging, so the guards get their chance to drop
		// them.
		release();
		await settled;

		await expect(providerSelect).toHaveValue('openai');
		await expect(page.locator('#llm-model')).toHaveValue('gpt-4o-mini');
		await expect(aiSecretState).toHaveText('Stored in settings');
		await expect(bringEmailChip).toHaveText('Stored in settings');

		// And the stored configuration is on the server.
		const res = await request.get('/api/settings');
		const snapshot = (await res.json()) as {
			ai: { provider: string; model: string; apiKey: { set: boolean } };
		};
		expect(snapshot.ai.provider).toBe('openai');
		expect(snapshot.ai.model).toBe('gpt-4o-mini');
		expect(snapshot.ai.apiKey.set).toBe(true);
	});

	test('given_settings_read_failed_when_the_stored_custom_provider_chosen_then_its_models_are_listed', async ({ page, request }) => {
		// The read failed, so the picker never learned the stored provider
		// (appliedProvider stays null) and treats selecting it as a selection
		// that changed nothing: the commit sends the provider alone and the
		// server answers with the stored endpoint and model. The listing must
		// run on that answer even for the custom provider, otherwise the model
		// select offers no option while a model is stored, and its only option
		// would commit `null` over the stored model.
		const stored = await request.patch('/api/settings', {
			data: {
				ai: { provider: 'custom', model: 'test-model', customBaseUrl: DEAD_ENDPOINT },
			},
		});
		expect(stored.ok()).toBe(true);

		// The listing succeeds, so the model select is rendered from its
		// options: a missing listing shows up as a blank select.
		await page.route('**/api/llm/models?*', (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['other-model', 'test-model'] }),
			})
		);
		// The mount read fails, so the stored provider is never learned from it.
		await page.route('**/api/settings*', async (route) => {
			if (route.request().method() !== 'GET') {
				await route.continue();
				return;
			}
			await route.fulfill({
				status: 500,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'boom', code: 'internal' }),
			});
		});

		await page.goto('/settings');
		const providerSelect = page.locator('select').first();
		await expect(providerSelect.locator('option[value="custom"]')).toHaveCount(1);
		await providerSelect.selectOption('custom');

		// The commit answer supplies the stored endpoint and model, and the
		// listing runs on it, so the stored model is one of the options.
		const modelSelect = page.locator('#llm-model');
		await expect(modelSelect).toHaveValue('test-model');
		await expect(modelSelect.locator('option[value="test-model"]')).toHaveCount(1);
		await expect(modelSelect.locator('option[value="other-model"]')).toHaveCount(1);
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
		// Scoped to the password field itself: the email field renders the same
		// chip, so an unscoped locator would stay green for the wrong field.
		const passwordField = page
			.locator('.settings-card')
			.nth(1)
			.locator('.field')
			.filter({ has: page.getByLabel('Bring! password') });
		await expect(passwordField.locator('.state-chip')).toHaveText('Stored in settings');
		await expect(page.locator('body')).not.toContainText('bring-secret-pass');
		// The body text cannot see an input's value, so pin the field itself.
		await expect(page.getByLabel('Bring! password')).toHaveValue('');

		const res = await request.get('/api/settings');
		expect(await res.text()).not.toContain('bring-secret-pass');
	});

	test('given_stored_bring_password_when_cleared_then_field_and_api_state_cleared', async ({ page, request }) => {
		// The Clear control is the only path that commits { bring: { password:
		// null } } and re-probes. A shell that exports BRING_PASSWORD feeds the
		// app through the environment, so every expectation is derived from what
		// the API reports after the clear instead of assuming the variable is
		// unset.
		const patch = await request.patch('/api/settings', {
			data: { bring: { email: 'cook@example.com', password: 'bring-secret-pass' } },
		});
		expect(patch.ok()).toBe(true);

		await page.goto('/settings');

		const bringCard = page.locator('.settings-card').nth(1);
		const passwordField = bringCard
			.locator('.field')
			.filter({ has: page.getByLabel('Bring! password') });
		// A stored password wins over the environment, so the chip shows it and
		// the Clear control is offered.
		await expect(passwordField.locator('.state-chip')).toHaveText('Stored in settings');

		await passwordField.getByRole('button', { name: 'Clear' }).click();

		// The commit answer is what moves the chip off "Stored in settings", so
		// waiting for that also proves the PATCH landed before the read below.
		await expect(passwordField.locator('.state-chip')).not.toHaveText('Stored in settings');

		const res = await request.get('/api/settings');
		const snapshot = (await res.json()) as {
			bring: { password: { set: boolean; source: string } };
		};
		// The stored value is gone; whether an environment value takes over is
		// the shell's business, not the assertion's.
		expect(snapshot.bring.password.source).not.toBe('settings');

		const reportedChip: Record<string, string> = {
			settings: 'Stored in settings',
			environment: 'Inherited from the environment',
			none: 'Not set',
		};
		await expect(passwordField.locator('.state-chip')).toHaveText(
			reportedChip[snapshot.bring.password.source]
		);
		// Nothing was typed into the field, so the clear leaves it empty.
		await expect(page.getByLabel('Bring! password')).toHaveValue('');
	});

	test('given_settings_read_failed_when_a_later_commit_succeeds_then_the_failure_notice_goes', async ({ page }) => {
		// The mount-time read fails and is reported. A later successful commit
		// proves the server answered with the current state, so the stale failure
		// notice and its retry must not linger beside a form that is loaded.
		let failReads = true;
		await page.route('**/api/settings*', (route) =>
			route.request().method() === 'GET' && failReads ? route.abort('failed') : route.continue()
		);
		await page.route('**/api/llm/models?*', (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['gpt-4o-mini'] }),
			})
		);

		await page.goto('/settings');
		await expect(page.locator('.llm-settings-error')).toContainText('Could not load the stored settings.');

		failReads = false;
		const providerSelect = page.locator('select').first();
		await expect(providerSelect.locator('option[value="openai"]')).toHaveCount(1);
		await providerSelect.selectOption('openai');

		await expect(page.locator('.llm-commit-state')).toHaveText('Saved');
		await expect(page.locator('.llm-settings-error')).toHaveCount(0);
	});

	test('given_bring_read_failed_when_a_later_commit_succeeds_then_the_failure_notice_goes', async ({ page }) => {
		// The page's own Bring! read fails while the picker's read of the same
		// endpoint succeeds, so the failure is reported on the card and nowhere
		// else. A later successful Bring! commit proves the server answered with
		// the current state, so the stale notice and its retry must not linger
		// beside fields that a fresh answer has already loaded.
		let reads = 0;
		let failReads = true;
		await page.route('**/api/settings*', (route) => {
			if (route.request().method() !== 'GET') return route.continue();
			reads++;
			if (reads === 1 || !failReads) return route.continue();
			// A non-ok answer fails the read at once; a transport error would
			// instead be retried twice by the API client.
			return route.fulfill({
				status: 503,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'service unavailable' }),
			});
		});

		await page.goto('/settings');

		const bringCard = page.locator('.settings-card').nth(1);
		const failure = bringCard.getByText('Could not load the stored settings.');
		await expect(failure).toBeVisible();
		// The AI read succeeded, so the picker has no failure of its own.
		await expect(page.locator('.llm-settings-error')).toHaveCount(0);

		failReads = false;
		const email = page.getByLabel('Bring! email');
		await email.fill('cook@example.com');
		await email.blur();

		// The commit answer is what loads the half, so waiting for its chip also
		// proves the answer was applied before the notice is judged.
		const passwordField = bringCard
			.locator('.field')
			.filter({ has: page.getByLabel('Bring! password') });
		await expect(passwordField.locator('.state-chip')).toBeVisible();
		await expect(failure).toHaveCount(0);
	});

	test('given_settings_read_fails_when_bring_state_unread_then_not_asserted_as_not_set', async ({ page }) => {
		await page.route('**/api/settings*', (route) =>
			route.request().method() === 'GET' ? route.abort('failed') : route.continue()
		);
		// The probe still answers: credentials exist but the endpoint rejects
		// them. That answer is the only Bring! state observed, so the failed
		// settings read must neither turn it into a different verdict nor let the
		// unread credential fields claim to be "Not set".
		await mockBringStatus(page, {
			configured: true,
			connected: false,
			error: 'Bring! login failed',
		});

		await page.goto('/settings');

		const bringCard = page.locator('.settings-card').nth(1);
		await expect(bringCard.locator('.state-chip').first()).toHaveText('Not connected');
		await expect(bringCard.getByText('Not set')).toHaveCount(0);
		await expect(bringCard.getByText('Stored in settings')).toHaveCount(0);
	});

	test('given_settings_read_fails_when_a_provider_chosen_then_no_key_state_claimed', async ({ page }) => {
		// The read failed, so the stored key was never observed, and the commit
		// that follows fails too: no snapshot ever applies. Until one does, the
		// picker must not answer "Not set" for a key that may well be stored, and
		// must not offer the Clear control that depends on knowing it is not.
		await page.route('**/api/settings*', (route) =>
			route.request().method() === 'GET'
				? route.abort('failed')
				: route.fulfill({
						status: 503,
						contentType: 'application/json',
						body: JSON.stringify({ error: 'service unavailable' }),
					})
		);
		await page.route('**/api/llm/models?*', (route) =>
			route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'API key not configured', code: 'llm_not_configured' }),
			})
		);

		await page.goto('/settings');

		const aiCard = page.locator('.settings-card').first();
		await expect(aiCard.getByText('Could not load the stored settings.')).toBeVisible();

		const providerSelect = page.locator('select').first();
		await expect(providerSelect.locator('option[value="openai"]')).toHaveCount(1);
		await providerSelect.selectOption('openai');

		// The choice reveals the field its key is stored in ...
		await expect(page.getByLabel('API key')).toBeVisible();
		// ... but neither the provenance chip nor the Clear control may appear
		// before an answer has said where the key comes from.
		await expect(aiCard.locator('.llm-secret-state')).toHaveCount(0);
		await expect(aiCard.getByText('Not set')).toHaveCount(0);
		await expect(aiCard.getByRole('button', { name: 'Clear' })).toHaveCount(0);
	});

	test('given_settings_read_failed_when_the_picker_retry_succeeds_then_the_bring_state_loads', async ({ page, request }) => {
		// The page reads the Bring! half and the picker reads the AI half, but
		// the picker's Retry is the only retry control on the page: a retry that
		// restored the AI section alone would leave the Bring! fields unread for
		// the rest of the visit.
		await request.patch('/api/settings', {
			data: { bring: { email: 'cook@example.com', password: 'bring-secret-pass' } },
		});

		let failReads = true;
		await page.route('**/api/settings*', (route) =>
			route.request().method() === 'GET' && failReads ? route.abort('failed') : route.continue()
		);

		await page.goto('/settings');
		const bringCard = page.locator('.settings-card').nth(1);
		const failure = page.locator('.llm-settings-error');
		await expect(failure).toContainText('Could not load the stored settings.');
		// The failed read observed nothing, so no chip may claim a provenance.
		await expect(bringCard.getByText('Stored in settings')).toHaveCount(0);

		failReads = false;
		await failure.getByRole('button', { name: 'Retry' }).click();

		await expect(page.locator('.llm-settings-error')).toHaveCount(0);
		await expect(page.getByLabel('Bring! email')).toHaveValue('cook@example.com');
		const passwordField = bringCard
			.locator('.field')
			.filter({ has: page.getByLabel('Bring! password') });
		await expect(passwordField.locator('.state-chip')).toHaveText('Stored in settings');
	});

	test('given_settings_read_fails_after_the_probe_answered_then_the_probe_verdict_stays', async ({ page }) => {
		// The settings read and the Bring! probe answer independently. The read
		// is held until the probe has spoken, so its failure can only be judged
		// against an answer that already exists.
		const gate = Promise.withResolvers<void>();
		await page.route('**/api/settings*', async (route) => {
			if (route.request().method() !== 'GET') {
				await route.continue();
				return;
			}
			await gate.promise;
			await route.abort('failed');
		});

		await page.goto('/settings');
		const bringCard = page.locator('.settings-card').nth(1);
		const bringChip = bringCard.locator('.state-chip').first();
		// The probe answered "no credentials anywhere" before the read failed.
		await expect(bringChip).toHaveText('Not configured');

		gate.resolve();
		// The read failure is reported by the picker...
		await expect(page.locator('.llm-settings-error')).toBeVisible();
		// ...and changes neither the probe's verdict nor the unread fields.
		await expect(bringChip).toHaveText('Not configured');
		await expect(bringCard.getByText('Not set')).toHaveCount(0);
	});

	test('given_a_slow_mount_probe_when_a_commit_reprobes_then_the_newer_verdict_wins', async ({ page }) => {
		// Probes are not serialized: the mount probe is held while a commit
		// lands and re-probes. The held (older) answer then arrives last, so
		// only the sequence guard keeps it from overwriting the newer verdict
		// and its inline error. The app bar probes the same endpoint and echoes
		// the answer in the footer, which makes the released stale answer
		// observable and proves it was processed before the page's verdict is
		// judged.
		let committed = false;
		const gate = Promise.withResolvers<void>();
		await page.route('**/api/bring/status', async (route) => {
			if (committed) {
				await route.fulfill({
					status: 200,
					contentType: 'application/json',
					body: JSON.stringify({
						configured: true,
						connected: false,
						error: 'Bring! login failed',
					}),
				});
				return;
			}
			// The held mount probe answers only once released, with a stale
			// verdict distinct from the one the commit produces.
			await gate.promise;
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					configured: true,
					connected: false,
					error: 'stale probe answer',
				}),
			});
		});
		await page.route('**/api/settings', async (route) => {
			const isCommit = route.request().method() === 'PATCH';
			const response = await route.fetch();
			if (isCommit) committed = true;
			await route.fulfill({ response });
		});

		await page.goto('/settings');
		const bringCard = page.locator('.settings-card').nth(1);
		const bringChip = bringCard.locator('.state-chip').first();
		// The held mount probe has not answered, so no verdict is claimed yet.
		await expect(bringChip).toHaveText('Checking…');

		// A commit lands and re-probes; the newer answer reports a rejected
		// connection.
		await page.getByLabel('Bring! email').fill('cook@example.com');
		await page.getByLabel('Bring! email').blur();
		const inlineError = page.getByRole('main').getByText('Bring! login failed');
		await expect(inlineError).toBeVisible();

		// The released mount probe is stale and must be dropped whole: the app
		// bar's footer echoes it, so waiting for that echo proves the released
		// answers were processed before the page's verdict is judged.
		gate.resolve();
		await expect(page.locator('.site-footer__bring-error')).toHaveText('stale probe answer');
		await expect(bringChip).toHaveText('Not connected');
		await expect(inlineError).toBeVisible();
	});

	test('given_the_ai_read_succeeds_and_the_bring_read_fails_then_the_card_reports_it_and_retries', async ({ page }) => {
		// The page reads the Bring! half and the picker reads the AI half from
		// the same endpoint, and the picker's read is issued first. Letting it
		// succeed while every page read fails leaves the Bring! card unloaded
		// with no picker error to explain it, so the card must report its own
		// failure and offer its own retry.
		let reads = 0;
		let failReads = true;
		await page.route('**/api/settings*', (route) => {
			if (route.request().method() !== 'GET') return route.continue();
			reads++;
			if (reads === 1 || !failReads) return route.continue();
			// A non-ok answer fails the read at once; a transport error would
			// instead be retried twice by the API client.
			return route.fulfill({
				status: 503,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'service unavailable' }),
			});
		});

		await page.goto('/settings');

		const bringCard = page.locator('.settings-card').nth(1);
		const failure = bringCard.getByText('Could not load the stored settings.');
		await expect(failure).toBeVisible();
		// The AI read succeeded, so the picker has no failure of its own.
		await expect(page.locator('.llm-settings-error')).toHaveCount(0);
		// Nothing was read, so no chip may claim a provenance.
		await expect(bringCard.getByText('Not set')).toHaveCount(0);

		// The card's own retry re-reads the Bring! half and loads the fields.
		failReads = false;
		await bringCard.getByRole('button', { name: 'Retry' }).click();
		await expect(failure).toHaveCount(0);
		const passwordField = bringCard
			.locator('.field')
			.filter({ has: page.getByLabel('Bring! password') });
		await expect(passwordField.locator('.state-chip')).toBeVisible();
	});

	test('given_the_ai_read_answered_first_when_the_page_loads_then_the_bring_half_is_read_once', async ({ page }) => {
		// The picker's read of the shared endpoint is issued first, so `aiLoaded`
		// flips while the page's own read is still unanswered. That must not start
		// a second read for the same page load: the half it would fetch is already
		// on its way, and its newer sequence number would make a failure of it
		// indistinguishable from a real one.
		//
		// Both reads are served concurrently, so the count alone would not prove
		// the guard works: when the page's read happens to be answered last the
		// count is 2 without any guard at all. Every read after the first is
		// therefore held until the AI half has been applied, which forces the
		// interleaving the guard exists for - a second read for this page load
		// would be issued while the page's own read is still unanswered.
		let reads = 0;
		const release = Promise.withResolvers<void>();
		await page.route('**/api/settings*', async (route) => {
			if (route.request().method() !== 'GET') return route.continue();
			reads++;
			if (reads === 1) return route.continue();
			await release.promise;
			return route.continue();
		});

		await page.goto('/settings');

		const aiCard = page.locator('.settings-card').first();
		const bringCard = page.locator('.settings-card').nth(1);
		// The AI half is loaded from the picker's answer, which the held read
		// cannot supply: at this point the page's own read is still unanswered.
		await expect(aiCard.locator('.state-chip').first()).toBeVisible();
		release.resolve();

		// ... and the Bring! half from the page's own read.
		const passwordField = bringCard
			.locator('.field')
			.filter({ has: page.getByLabel('Bring! password') });
		await expect(passwordField.locator('.state-chip')).toBeVisible();

		// Both halves are loaded, so any further read would already have been
		// issued: exactly the two reads this page load needs.
		expect(reads).toBe(2);
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

	test('given_no_stored_password_when_settings_load_then_password_field_matches_the_reported_state', async ({ page, request }) => {
		// A shell that exports BRING_PASSWORD feeds the app through the
		// environment, so the expectation is derived from what the API reports
		// instead of assuming the variable is unset: with neither a stored nor
		// an inherited password there is nothing to replace and the field asks
		// for a new one, while an effective password must make it offer a
		// replacement.
		const res = await request.get('/api/settings');
		const snapshot = (await res.json()) as {
			bring: { password: { set: boolean; source: string } };
		};

		await page.goto('/settings');

		// The provenance chip is rendered from the snapshot and not from the
		// pre-read default (which says "Not set"), so it proves the read was
		// applied before the placeholder below is judged.
		const passwordField = page
			.locator('.settings-card')
			.nth(1)
			.locator('.field')
			.filter({ has: page.getByLabel('Bring! password') });
		const reportedChip: Record<string, string> = {
			settings: 'Stored in settings',
			environment: 'Inherited from the environment',
			none: 'Not set',
		};
		await expect(passwordField.locator('.state-chip')).toHaveText(
			reportedChip[snapshot.bring.password.source]
		);

		await expect(page.getByLabel('Bring! password')).toHaveAttribute(
			'placeholder',
			snapshot.bring.password.set
				? 'Enter a new value to replace the current one'
				: 'Enter your Bring! password'
		);
	});

	test('given_settings_read_still_pending_when_generate_page_opened_then_no_unconfigured_notice', async ({ page }) => {
		// A slow link: the stored configuration is held unanswered, so the page
		// must not report the install as unconfigured while it waits.
		let reads = 0;
		const gate = Promise.withResolvers<void>();
		await page.route('**/api/settings*', async (route) => {
			if (route.request().method() !== 'GET') {
				await route.continue();
				return;
			}
			const response = await route.fetch();
			reads++;
			await gate.promise;
			await route.fulfill({ response });
		});

		await page.goto('/spontaneous');
		await expect.poll(() => reads).toBeGreaterThan(0);
		await expect(page.locator('.ai-config-notice')).toHaveCount(0);

		gate.resolve();

		// The answer lands and the fresh database reports no provider, so the
		// notice belongs on screen now.
		await expect(page.locator('.ai-config-notice')).toBeVisible();
	});

	test('given_settings_read_fails_when_generate_page_opened_then_retry_replaces_the_notice', async ({ page }) => {
		await page.route('**/api/settings*', (route) =>
			route.request().method() === 'GET' ? route.abort('failed') : route.continue()
		);

		await page.goto('/spontaneous');

		const failure = page.locator('.llm-settings-error');
		await expect(failure).toContainText('Could not load the stored settings.');
		// The server may hold a complete configuration, so a failed read must
		// never be reported as "no AI provider configured".
		await expect(page.locator('.ai-config-notice')).toHaveCount(0);

		// The read is retryable from the picker itself.
		await page.unroute('**/api/settings*');
		await failure.getByRole('button', { name: 'Retry' }).click();
		await expect(page.locator('.llm-settings-error')).toHaveCount(0);
		await expect(page.locator('.ai-config-notice')).toBeVisible();
	});

	test('given_no_ai_configuration_when_generate_page_opened_then_notice_links_to_settings', async ({ page }) => {
		await page.goto('/spontaneous');
		const notice = page.locator('.ai-config-notice');
		await expect(notice).toBeVisible();
		await notice.getByRole('link', { name: 'Open settings' }).click();
		await expect(page).toHaveURL(/\/settings$/);
	});
});
