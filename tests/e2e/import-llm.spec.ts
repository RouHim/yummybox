import { test, expect, type Page } from '@playwright/test';
import { resetMeals, resetSettings, setLocale } from './_helpers';

test.describe('LLM import', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
		await resetSettings(request);
	});

	async function openLlmTab(page: Page): Promise<void> {
		await page.goto('/meals');
		await page.getByRole('button', { name: /^Add meal$|^Mahlzeit hinzufügen$/ }).click();
		await expect(page.getByRole('dialog')).toBeVisible();
		await page.getByRole('button', { name: 'AI import' }).click();
	}

	test('given_llm_provider_configured_when_parse_clicked_then_form_populated_and_meal_added', async ({ page }) => {
		await page.route('**/api/llm/providers', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					providers: [
						{
							id: 'openai',
							name: 'OpenAI',
							envVar: 'OPENAI_API_KEY',
							configured: true,
							supportsCustomEndpoint: false,
						},
					],
				}),
			});
		});

		await page.route('**/api/llm/models?*', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['gpt-4o-mini'] }),
			});
		});

		await page.route('**/api/import/llm', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					name: 'AI Curry',
					ingredients: [{ name: 'chicken', quantity: '200 g' }],
					instructions: 'Cook.',
					imageBase64: null,
				}),
			});
		});

		await openLlmTab(page);

		const dialog = page.getByRole('dialog');
		const providerSelect = dialog.locator('select').first();
		await expect(providerSelect).toBeVisible();
		await providerSelect.selectOption('openai');

		const modelSelect = dialog.locator('select').nth(1);
		await expect(modelSelect).toBeVisible();
		await modelSelect.selectOption('gpt-4o-mini');

		// `openai` needs a stored key: the parse button stays disabled until the
		// stored configuration is complete. Blurring commits the key.
		const apiKey = dialog.getByLabel('API key');
		await apiKey.fill('test-key');
		await apiKey.blur();

		await dialog.locator('.llm-hint-input').fill('A spicy chicken curry');

		await page.getByRole('button', { name: 'Parse with AI' }).click();

		// After LLM import, form should be visible with populated data (auto-switched to manual tab)
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('AI Curry');

		await dialog.getByRole('button', { name: /^(Add|Hinzufügen)$/ }).click();

		await expect(dialog).not.toBeVisible();
		await expect(page.getByRole('listitem').filter({ hasText: 'AI Curry' })).toBeVisible();
	});

	test('given_no_llm_providers_when_llm_tab_opened_then_error_shown', async ({ page }) => {
		await page.route('**/api/llm/providers', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ providers: [] }),
			});
		});

		await openLlmTab(page);

		const error = page.locator('.form-error').filter({ hasText: /No LLM providers available/ });
		await expect(error).toBeVisible();
		await expect(page.getByRole('button', { name: 'Parse with AI' })).toHaveCount(0);
	});

	test('given_failed_model_listing_when_api_key_stored_then_model_select_appears', async ({ page }) => {
		await page.route('**/api/llm/providers', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					providers: [
						{
							id: 'openai',
							name: 'OpenAI',
							envVar: 'OPENAI_API_KEY',
							configured: true,
							supportsCustomEndpoint: false,
						},
					],
				}),
			});
		});

		// The server resolves the key from the store: the listing only succeeds
		// once the key has been committed, exactly like the real backend. The
		// decision uses the key's *source*, not `set`: `set` is also true for an
		// ambient OPENAI_API_KEY in the environment, which would flip this mock
		// to 200 before the test has stored anything.
		await page.route('**/api/llm/models?*', async (route) => {
			const settings = await page.request.get('/api/settings');
			const keySource = ((await settings.json()) as { ai: { apiKey: { source: string } } }).ai.apiKey
				.source;
			if (keySource !== 'settings') {
				await route.fulfill({
					status: 400,
					contentType: 'application/json',
					body: JSON.stringify({ error: 'API key not configured', code: 'llm_not_configured' }),
				});
				return;
			}
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['gpt-4o-mini'] }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		await dialog.locator('select').first().selectOption('openai');

		// A listing without a stored key falls back to a free-text model field.
		const modelInput = dialog.getByPlaceholder('Model name (e.g. gpt-4o-mini)');
		await expect(modelInput).toBeVisible();

		// Storing the key must retry the listing and bring the select back.
		const apiKey = dialog.getByLabel('API key');
		await apiKey.fill('test-key');
		await apiKey.blur();

		const modelSelect = dialog.locator('select').nth(1);
		await expect(modelSelect).toBeVisible();
		await expect(modelSelect.locator('option[value="gpt-4o-mini"]')).toHaveCount(1);
	});

	test('given_loaded_custom_models_when_base_url_cleared_then_stale_models_are_dropped', async ({ page }) => {
		await page.route('**/api/llm/providers', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					providers: [
						{
							id: 'custom',
							name: 'Custom (OpenAI-compatible)',
							envVar: '',
							configured: true,
							supportsCustomEndpoint: true,
						},
					],
				}),
			});
		});

		await page.route('**/api/llm/models?*', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['local-model'] }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		await dialog.locator('select').first().selectOption('custom');
		await dialog.getByLabel('Base URL').fill('http://127.0.0.1:18999/v1/');
		await dialog.getByLabel('Base URL').blur();

		const modelSelect = dialog.locator('select').nth(1);
		await expect(modelSelect.locator('option[value="local-model"]')).toHaveCount(1);
		await modelSelect.selectOption('local-model');

		// Clearing the endpoint drops the models that belonged to it.
		await dialog.getByLabel('Base URL').fill('');
		await dialog.getByLabel('Base URL').blur();

		await expect(dialog.locator('select').nth(1).locator('option[value="local-model"]')).toHaveCount(0);
	});

	test('given_a_stored_key_when_the_picker_remounts_then_its_own_read_decides_the_state', async ({ page, request }) => {
		// The import dialog's picker remounts on a tab switch (and on collapse)
		// while the caller keeps the `loaded` flag of the earlier mount. The key
		// chip is gated on that flag, so without resetting it the remount reports
		// "Not set" and hides Clear for a key the server has stored, before its
		// own read has answered. Every read after the first fails here, so a
		// false claim would persist for the whole mount.
		const stored = await request.patch('/api/settings', {
			data: { ai: { provider: 'openai', model: 'gpt-4o-mini', apiKey: 'sk-stored' } },
		});
		expect(stored.ok()).toBe(true);

		// No listing may leave the machine: the stored key is not a real one.
		await page.route('**/api/llm/models?*', async (route) => {
			await route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'API key not configured', code: 'llm_not_configured' }),
			});
		});

		let reads = 0;
		let failReads = false;
		await page.route('**/api/settings*', (route) => {
			if (route.request().method() !== 'GET') return route.continue();
			reads++;
			if (!failReads) return route.continue();
			// A non-ok answer fails the read at once; a transport error would
			// instead be retried twice by the API client.
			return route.fulfill({
				status: 503,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'service unavailable' }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		// The key's own chip class, so the assertions below cannot silently
		// attach to another field of the picker.
		const aiKeyChip = dialog.locator('.llm-secret-state');
		const clearKey = dialog.getByRole('button', { name: 'Clear' });
		// The mount read answers, so the chip reports the stored key.
		await expect(aiKeyChip).toHaveText('Stored in settings');
		await expect(clearKey).toBeVisible();
		expect(reads).toBe(1);

		// Leaving the tab unmounts the picker; coming back remounts it with the
		// caller's `loaded` still true from the mount that just answered.
		await dialog.getByRole('button', { name: 'Manual' }).click();
		failReads = true;
		await dialog.getByRole('button', { name: 'AI import' }).click();

		// The remount really rendered its fields: without these the chip
		// assertion below would pass on a picker that has not mounted at all.
		await expect(dialog.locator('select').first()).toHaveValue('openai');
		await expect(dialog.getByLabel('API key')).toBeVisible();

		// Its own read failed, so nothing may be claimed about the key: the chip
		// and the Clear button stay away instead of reporting "Not set".
		await expect(dialog.getByText('Could not load the stored settings.')).toBeVisible();
		await expect(aiKeyChip).toHaveCount(0);
		await expect(clearKey).toHaveCount(0);

		// And the false claim never became true: the key is still stored.
		const res = await request.get('/api/settings');
		const snapshot = (await res.json()) as { ai: { apiKey: { set: boolean; source: string } } };
		expect(snapshot.ai.apiKey).toEqual({ set: true, source: 'settings' });
	});

	test('given_a_stored_provider_when_the_picker_remounts_then_its_models_are_listed_once', async ({ page, request }) => {
		// A remount starts with the caller's provider already set, so the
		// remount's own listing and the one that follows the read would both run
		// for the same state: two requests where one answers it, and for the
		// custom provider a first listing without the stored endpoint that the
		// read still has to supply.
		const stored = await request.patch('/api/settings', {
			data: { ai: { provider: 'openai', model: 'gpt-4o-mini', apiKey: 'sk-stored' } },
		});
		expect(stored.ok()).toBe(true);

		let listings = 0;
		await page.route('**/api/llm/models?*', async (route) => {
			listings++;
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['gpt-4o-mini'] }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		// The mount read supplies the stored provider, and its listing fills the
		// model select: the option is the listing's own evidence.
		await expect(dialog.locator('#llm-model')).toHaveValue('gpt-4o-mini');
		expect(listings).toBe(1);

		// Leaving the tab unmounts the picker; coming back remounts it with the
		// caller's provider still set.
		await dialog.getByRole('button', { name: 'Manual' }).click();
		listings = 0;
		await dialog.getByRole('button', { name: 'AI import' }).click();

		// The key chip reports the stored key only once this mount's read has
		// answered, and a listing that answer triggers is issued before it is
		// rendered: waiting for the chip therefore also waits for the duplicate
		// this test guards against.
		await expect(dialog.locator('.llm-secret-state')).toHaveText('Stored in settings');
		await expect(dialog.locator('#llm-model')).toHaveValue('gpt-4o-mini');
		// A window for a stray request to arrive before the count is judged: a
		// second listing is issued in the same turn as the answer, so it has
		// landed long before this returns.
		await expect
			.poll(async () => {
				await page.waitForTimeout(200);
				return listings;
			}, { timeout: 5_000 })
			.toBe(1);
	});

	test('given_a_stored_custom_provider_when_the_picker_remounts_then_its_endpoint_is_read_before_the_listing', async ({ page, request }) => {
		// A remount starts with the caller's provider already set but without the
		// custom endpoint, which the caller does not bind. A listing that runs
		// before this mount's own read answers therefore sees an empty URL,
		// returns early and never re-runs: the model select would stay empty
		// while the stored model is on the server. Waiting for the read is what
		// makes the listing run once, on the endpoint the answer supplies. The
		// stored provider is `custom` because only there does the listing depend
		// on the read's answer; with `openai` this test would pass without the
		// gate.
		const stored = await request.patch('/api/settings', {
			data: {
				ai: {
					provider: 'custom',
					model: 'custom-model',
					customBaseUrl: 'http://127.0.0.1:1/v1/',
				},
			},
		});
		expect(stored.ok()).toBe(true);

		let listings = 0;
		await page.route('**/api/llm/models?*', async (route) => {
			listings++;
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['custom-model', 'other-model'] }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		// The mount read supplies the stored provider, endpoint and model, and
		// its listing fills the model select.
		await expect(dialog.locator('select').first()).toHaveValue('custom');
		await expect(dialog.getByLabel('Base URL')).toHaveValue('http://127.0.0.1:1/v1/');
		await expect(dialog.locator('#llm-model')).toHaveValue('custom-model');
		expect(listings).toBe(1);

		// Leaving the tab unmounts the picker; coming back remounts it with the
		// caller's provider still set and the endpoint reset.
		await dialog.getByRole('button', { name: 'Manual' }).click();
		listings = 0;
		await dialog.getByRole('button', { name: 'AI import' }).click();

		// The remount lists only once its own read has answered, so the listing's
		// model is an option and the stored one is selected.
		const modelSelect = dialog.locator('#llm-model');
		await expect(modelSelect.locator('option[value="custom-model"]')).toHaveCount(1);
		await expect(modelSelect).toHaveValue('custom-model');
		// A window for a stray request to arrive before the count is judged: a
		// second listing is issued in the same turn as the answer, so it has
		// landed long before this returns.
		await expect
			.poll(async () => {
				await page.waitForTimeout(200);
				return listings;
			}, { timeout: 5_000 })
			.toBe(1);
	});

	test('given_a_cleared_endpoint_when_the_remount_read_lands_first_then_the_endpoint_stays_cleared', async ({ page, request }) => {
		// A remount starts with the caller's provider still set but without the
		// custom endpoint, which the caller does not bind: the field shows
		// nothing while this mount's own read is in flight, so the user commits
		// the empty value (a clear) before that read answers. The read captured
		// the endpoint the clear removes, so filling the emptied field from it
		// would show an endpoint the server no longer has - and the clear's own
		// answer then skips the same fill, because the field is no longer empty.
		// The picker would look configured while its chip and the AI notice say
		// otherwise, blocking the flows behind that notice.
		const stored = await request.patch('/api/settings', {
			data: {
				ai: {
					provider: 'custom',
					model: 'custom-model',
					customBaseUrl: 'http://127.0.0.1:1/v1/',
				},
			},
		});
		expect(stored.ok()).toBe(true);
		// No listing may leave the machine for the closed endpoint.
		await page.route('**/api/llm/models?*', (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['custom-model'] }),
			})
		);

		// The first mount's reads pass through; the remount's own read is held,
		// and so is the clear commit's answer, so the read is answered first.
		let holdReads = false;
		let holdCommits = false;
		const readGate = Promise.withResolvers<void>();
		const readHeld = Promise.withResolvers<void>();
		const commitGate = Promise.withResolvers<void>();
		const commitHeld = Promise.withResolvers<void>();
		await page.route('**/api/settings*', async (route) => {
			const method = route.request().method();
			if (method === 'GET') {
				if (!holdReads) {
					await route.continue();
					return;
				}
				// Fetched up front, so the held answer is the endpoint as it was
				// when the read was issued: the one the clear below removes.
				const response = await route.fetch();
				readHeld.resolve();
				await readGate.promise;
				await route.fulfill({ response });
				return;
			}
			if (holdCommits) {
				const response = await route.fetch();
				commitHeld.resolve();
				await commitGate.promise;
				await route.fulfill({ response });
				return;
			}
			await route.continue();
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		// The first mount's read supplies the stored provider and endpoint.
		await expect(dialog.getByLabel('Base URL')).toHaveValue('http://127.0.0.1:1/v1/');

		// Leaving the tab unmounts the picker; coming back remounts it with the
		// caller's provider still set, an empty endpoint field and its own read
		// held. The key chip is dropped by that read's reset of `loaded`, so its
		// return below proves the read landed.
		holdReads = true;
		await dialog.getByRole('button', { name: 'Manual' }).click();
		await dialog.getByRole('button', { name: 'AI import' }).click();
		await readHeld.promise;
		const baseUrl = dialog.getByLabel('Base URL');
		await expect(baseUrl).toHaveValue('');
		await expect(dialog.locator('.llm-secret-state')).toHaveCount(0);

		// Enter commits the empty field as a clear without needing it to lose
		// focus first; its answer is held back so the read is answered first.
		holdCommits = true;
		await baseUrl.press('Enter');
		await commitHeld.promise;
		readGate.resolve();
		await expect(dialog.locator('.llm-secret-state')).toHaveCount(1);
		commitGate.resolve();

		await expect(dialog.locator('.llm-commit-state')).toHaveText('Saved');
		// The clear stands: the field shows nothing, never the endpoint the read
		// still carried, and the section reports itself unconfigured.
		await expect(baseUrl).toHaveValue('');
		await expect(dialog.locator('.ai-config-notice')).toBeVisible();

		const after = await request.get('/api/settings');
		const snapshot = (await after.json()) as { ai: { customBaseUrl: string; model: string } };
		expect(snapshot.ai.customBaseUrl).toBe('');
		expect(snapshot.ai.model).toBe('');
	});
});
