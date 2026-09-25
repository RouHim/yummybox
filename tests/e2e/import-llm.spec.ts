import { test, expect, type Page } from '@playwright/test';
import { resetMeals, resetSettings, setLocale } from './_helpers';

test.describe('LLM import', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
		await resetSettings(request);
		// The app bar probes the Bring! status on every mount, and the server
		// resolves the stored-then-environment credentials for it: on a shell
		// that exports BRING_EMAIL/BRING_PASSWORD that probe would log in to the
		// real Bring! API. Answer it locally so the suite stays hermetic.
		await page.route('**/api/bring/status', (route) =>
			route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ configured: false, connected: false, error: null }),
			})
		);
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

		// `openai` resolves its key from the store, so this test drives the flow
		// where the key is pasted and then used by the parse request. The parse
		// button is already enabled here because provider and model are set
		// locally (`importLlmLocallyReady`), so the key commit is incidental to
		// its enablement. Blurring commits the key.
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

	test('given_a_model_committed_by_the_parse_click_then_the_import_waits_for_it', async ({ page }) => {
		// The same race as on the generate page: choosing the model is part of
		// the click that completes the configuration, and that click also sends
		// the import request. A request that overtakes the model's commit is
		// refused with `llm_not_configured`, so the import must wait for it.
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

		// Hold the model's commit, and count the import requests: without the
		// wait the request leaves while that commit is still unanswered.
		const storedModel = Promise.withResolvers<void>();
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"model":"gpt-4o-mini"')) {
				await route.continue();
				return;
			}
			await storedModel.promise;
			await route.continue();
		});
		// Fulfilled like the sibling test's route: the configuration stored here
		// is `openai` with the bogus key below, so continuing this request would
		// put a real provider call on every run of this suite.
		let imports = 0;
		await page.route('**/api/import/llm', async (route) => {
			imports++;
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
		await dialog.locator('select').first().selectOption('openai');
		const modelSelect = dialog.locator('select').nth(1);
		await expect(modelSelect).toBeVisible();
		const apiKey = dialog.getByLabel('API key');
		await apiKey.fill('test-key');
		await apiKey.blur();
		await dialog.locator('.llm-hint-input').fill('A spicy chicken curry');

		await modelSelect.selectOption('gpt-4o-mini');
		const parse = page.getByRole('button', { name: 'Parse with AI' });
		await expect(parse).toBeEnabled();
		await parse.click();

		await page.waitForTimeout(300);
		expect(imports).toBe(0);

		storedModel.resolve();
		await expect.poll(() => imports).toBe(1);
		// The fulfilled answer is the draft on screen: the request count above
		// says an import ran, this says it was the one that populated the form.
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('AI Curry');
	});

	test('given_a_model_committed_on_the_collapse_then_the_import_waits_for_it', async ({ page }) => {
		// Hiding the settings block is the click that also ends the interaction
		// which issued the model's commit. Destroying the picker there left the
		// parse click with nothing to wait on, so the import overtook the write
		// and the user who had just picked the model was told the AI is not
		// configured.
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

		// Hold the model's commit, and count the import requests: without the
		// wait the request leaves while that commit is still unanswered.
		const storedModel = Promise.withResolvers<void>();
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"model":"gpt-4o-mini"')) {
				await route.continue();
				return;
			}
			await storedModel.promise;
			await route.continue();
		});
		let imports = 0;
		await page.route('**/api/import/llm', async (route) => {
			imports++;
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
		await dialog.locator('select').first().selectOption('openai');
		const modelSelect = dialog.locator('select').nth(1);
		await expect(modelSelect).toBeVisible();
		await modelSelect.selectOption('gpt-4o-mini');

		// The collapse belongs to the same interaction as the commit: the
		// model's answer is still in flight when the picker is hidden.
		await dialog.getByRole('button', { name: 'Hide' }).click();
		await expect(modelSelect).toBeHidden();
		await dialog.locator('.llm-hint-input').fill('A spicy chicken curry');
		const parse = page.getByRole('button', { name: 'Parse with AI' });
		await expect(parse).toBeEnabled();
		await parse.click();

		await page.waitForTimeout(300);
		expect(imports).toBe(0);

		storedModel.resolve();
		await expect.poll(() => imports).toBe(1);
	});

	test('given_a_model_committed_before_a_tab_switch_then_the_import_waits_for_it', async ({ page }) => {
		// Leaving the AI tab unmounts the picker, and the commit a click issued
		// just before it has to stay awaitable: a remount brings a fresh
		// instance whose own wait knows nothing about the predecessor's write,
		// so the import click would overtake it and be refused with
		// `llm_not_configured`.
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

		const storedModel = Promise.withResolvers<void>();
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"model":"gpt-4o-mini"')) {
				await route.continue();
				return;
			}
			await storedModel.promise;
			await route.continue();
		});
		let imports = 0;
		await page.route('**/api/import/llm', async (route) => {
			imports++;
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
		await dialog.locator('select').first().selectOption('openai');
		const modelSelect = dialog.locator('select').nth(1);
		await expect(modelSelect).toBeVisible();
		await modelSelect.selectOption('gpt-4o-mini');

		// Away and back: the instance that issued the model's commit is gone by
		// the time the parse click runs, and its answer is still in flight.
		await dialog.getByRole('button', { name: 'Manual' }).click();
		await dialog.getByRole('button', { name: 'AI import' }).click();
		await dialog.locator('.llm-hint-input').fill('A spicy chicken curry');
		const parse = page.getByRole('button', { name: 'Parse with AI' });
		await expect(parse).toBeEnabled();
		await parse.click();

		await page.waitForTimeout(300);
		expect(imports).toBe(0);

		storedModel.resolve();
		await expect.poll(() => imports).toBe(1);
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

		// Hold the model's commit, and the clear's own answer: the clear has to
		// land while the model's answer is still in flight, because only then can
		// that older answer refill a field the newer commit emptied. Waiting for
		// the model to be saved first (as this test used to) removes the very
		// window the fill guard exists for, and the test then passes with the
		// guard deleted.
		const storedModel = Promise.withResolvers<void>();
		const clearSent = Promise.withResolvers<void>();
		const clearGate = Promise.withResolvers<void>();
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH') {
				await route.continue();
				return;
			}
			if (body.includes('"customBaseUrl":null') && !body.includes('"provider"')) {
				// The clear of the endpoint, whose answer is held so the stale
				// model answer is applied while this one is still outstanding.
				clearSent.resolve();
				await clearGate.promise;
				await route.continue();
				return;
			}
			if (!body.includes('"model":"local-model"')) {
				await route.continue();
				return;
			}
			await storedModel.promise;
			await route.continue();
		});

		await modelSelect.selectOption('local-model');
		// Clearing the endpoint drops the models that belonged to it.
		await dialog.getByLabel('Base URL').fill('');
		await dialog.getByLabel('Base URL').blur();

		// The clear is queued behind the model's commit, so its request leaves
		// only once that answer has been applied and any refill would have
		// happened.
		storedModel.resolve();
		await clearSent.promise;
		// The clear's answer is still held: whatever the field shows now comes
		// from the model's pre-clear body.
		await expect(dialog.getByLabel('Base URL')).toHaveValue('');
		await expect(modelSelect.locator('option[value="local-model"]')).toHaveCount(0);

		clearGate.resolve();
		await expect(dialog.locator('.llm-commit-state')).toHaveText('Saved');
		await expect(dialog.getByLabel('Base URL')).toHaveValue('');
	});

	test('given_a_rejected_provider_commit_when_the_stored_one_is_reselected_then_the_model_fills_back', async ({ page, request }) => {
		// A rejection is answered like any other commit. Counting only successes
		// consumed an issue number that no answer would ever match, so after one
		// failed write the two counters could no longer meet in that mount and
		// the fill that stops a selection which changed nothing from deleting the
		// stored values was dead for good. The switch to `custom` is rejected
		// here, and re-selecting the stored `openai` sends the provider alone, so
		// the server keeps its model: the answer has to fill it back.
		const stored = await request.patch('/api/settings', {
			data: { ai: { provider: 'openai', model: 'gpt-4o-mini', apiKey: 'sk-stored' } },
		});
		expect(stored.ok()).toBe(true);

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
				body: JSON.stringify({ models: ['gpt-4o-mini'] }),
			});
		});
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"provider":"custom"')) {
				await route.continue();
				return;
			}
			await route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'provider unavailable', code: 'validation' }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		const providerSelect = dialog.locator('select').first();
		// The mount read supplies the stored provider and its model.
		await expect(providerSelect).toHaveValue('openai');
		const modelSelect = dialog.locator('select').nth(1);
		await expect(modelSelect).toHaveValue('gpt-4o-mini');

		// The switch empties the dependent fields locally and its commit fails.
		await providerSelect.selectOption('custom');
		await expect(dialog.locator('.llm-commit-state')).toHaveText('Could not save');
		await expect(modelSelect).toHaveValue('');

		// Back to the stored provider: the provider alone is sent, so the server
		// keeps the model, and its answer must show it again.
		await providerSelect.selectOption('openai');
		await expect(modelSelect).toHaveValue('gpt-4o-mini');
	});

	test('given_a_rejected_model_commit_when_enter_pressed_then_the_model_is_stored', async ({ page, request }) => {
		// A select cannot re-fire `change` for the value it already shows, so a
		// rejected model commit would leave the control unable to resend it -
		// and the provider-gated Retry stays hidden because the provider itself
		// is stored. Enter must retry it, as it does on every other control.
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

		let rejectModel = true;
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"model":"local-model"')) {
				await route.continue();
				return;
			}
			if (!rejectModel) {
				await route.continue();
				return;
			}
			await route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'model unavailable', code: 'validation' }),
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
		await expect(dialog.locator('.llm-commit-state')).toHaveText('Could not save');

		// Retry without moving the selection off the rejected value.
		rejectModel = false;
		await modelSelect.press('Enter');
		await expect(dialog.locator('.llm-commit-state')).toHaveText('Saved');

		const res = await request.get('/api/settings');
		const snapshot = (await res.json()) as { ai: { model: string } };
		expect(snapshot.ai.model).toBe('local-model');
	});

	test('given_a_rejected_switch_when_a_listing_was_in_flight_then_the_model_control_is_usable', async ({ page }) => {
		// The switch invalidates the listing in flight, and a superseded
		// request's `finally` deliberately leaves the loading flag alone so its
		// successor owns it. A rejected switch starts no successor, so the flag
		// must be cleared where the rejection is handled: otherwise the model
		// control stays a disabled select reading "Loading models…" until the
		// selection is committed again.
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
						{ id: 'openai', name: 'OpenAI', envVar: 'OPENAI_API_KEY', configured: true },
					],
				}),
			});
		});

		// Hold the custom listing so the switch lands while it is in flight.
		const listingGate = Promise.withResolvers<void>();
		const listingHeld = Promise.withResolvers<void>();
		const listingAnswered = Promise.withResolvers<void>();
		await page.route('**/api/llm/models?*', async (route) => {
			const provider = new URL(route.request().url()).searchParams.get('provider');
			if (provider !== 'custom') {
				await route.fulfill({
					status: 200,
					contentType: 'application/json',
					body: JSON.stringify({ models: ['gpt-4o-mini'] }),
				});
				return;
			}
			listingHeld.resolve();
			await listingGate.promise;
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['local-model'] }),
			});
			listingAnswered.resolve();
		});

		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"provider":"openai"')) {
				await route.continue();
				return;
			}
			await route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'provider unavailable', code: 'validation' }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		const providerSelect = dialog.locator('select').first();
		await providerSelect.selectOption('custom');
		await dialog.getByLabel('Base URL').fill('http://127.0.0.1:18999/v1/');
		await dialog.getByLabel('Base URL').blur();
		await listingHeld.promise;

		await providerSelect.selectOption('openai');
		await expect(dialog.locator('.llm-commit-state')).toHaveText('Could not save');

		// The control must accept a model again; the held listing's late answer
		// must not disable it either.
		await expect(dialog.locator('select').nth(1)).toBeEnabled();
		listingGate.resolve();
		// Let the held answer land before the test ends, so it cannot outlive
		// the route it is fulfilled on.
		await listingAnswered.promise;
		await expect(dialog.locator('select').nth(1)).toBeEnabled();
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

	test('given_a_rejected_provider_commit_when_the_picker_remounts_then_the_unstored_provider_is_not_listed', async ({ page, request }) => {
		// The select can end up showing a provider the server does not have: its
		// commit was rejected, so `appliedProvider` still names the stored one.
		// A remount resets the picker's listing memo, and listing then would query
		// the shown provider through the stored provider's credentials, so a
		// model picked from that list is committed for a provider the user never
		// saw configured.
		const stored = await request.patch('/api/settings', {
			data: { ai: { provider: 'openai', model: 'gpt-4o-mini', apiKey: 'sk-stored' } },
		});
		expect(stored.ok()).toBe(true);

		let rejectSwitch = true;
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"provider":"anthropic"')) {
				await route.continue();
				return;
			}
			if (!rejectSwitch) {
				await route.continue();
				return;
			}
			await route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'provider unavailable', code: 'validation' }),
			});
		});

		const listed: string[] = [];
		await page.route('**/api/llm/models?*', async (route) => {
			listed.push(new URL(route.request().url()).searchParams.get('provider') ?? '');
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ models: ['claude-3-5-haiku'] }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		const providerSelect = dialog.locator('select').first();
		await expect(providerSelect).toHaveValue('openai');
		await providerSelect.selectOption('anthropic');
		await expect(dialog.locator('.llm-commit-state')).toHaveText('Could not save');

		// Leaving the tab unmounts the picker; coming back remounts it with the
		// caller's rejected selection still bound.
		await dialog.getByRole('button', { name: 'Manual' }).click();
		listed.length = 0;
		await dialog.getByRole('button', { name: 'AI import' }).click();
		await expect(providerSelect).toHaveValue('anthropic');
		// The key chip renders only once this mount's own read has answered, and
		// the listing effect runs on that answer: waiting for the chip is
		// waiting for the picker's chance to list.
		await expect(dialog.locator('.llm-secret-state')).toHaveText('Stored in settings');
		// A window for a stray request to land before the count is judged.
		await page.waitForTimeout(300);
		expect(listed).toEqual([]);
		// The model control is there to receive the listing, so the empty result
		// above is not an artifact of a picker that never mounted its fields.
		await expect(dialog.locator('#llm-model')).toBeVisible();

		// Enter re-sends the rejected selection, which is the way back: the
		// provider is stored and its own models are listed.
		rejectSwitch = false;
		await providerSelect.press('Enter');
		await expect.poll(() => listed).toEqual(['anthropic']);
		// The selection that was just stored carries no model of its own, so the
		// listing's model is an option to pick rather than the selected value.
		await expect(dialog.locator('#llm-model option[value="claude-3-5-haiku"]')).toHaveCount(1);
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

	test('given_a_held_switch_when_a_second_write_is_queued_then_the_stored_switch_still_lists_its_models', async ({ page, request }) => {
		// The post-commit callbacks decided from the committer's shared state.
		// A successor queued behind a commit re-enters that state as `saving`
		// before the earlier callback runs, so a stored switch was read as a
		// rejection and returned early: the new provider's models were never
		// listed and the model control stayed an empty select even though the
		// server had stored the switch. The commit's own outcome is the only
		// value that answers what happened to that write.
		const stored = await request.patch('/api/settings', {
			data: { ai: { provider: 'openai', model: 'gpt-4o-mini', apiKey: 'sk-stored' } },
		});
		expect(stored.ok()).toBe(true);

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
						{
							id: 'anthropic',
							name: 'Anthropic',
							envVar: 'ANTHROPIC_API_KEY',
							configured: true,
							supportsCustomEndpoint: false,
						},
					],
				}),
			});
		});

		const listed: string[] = [];
		await page.route('**/api/llm/models?*', async (route) => {
			const provider = new URL(route.request().url()).searchParams.get('provider') ?? '';
			listed.push(provider);
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					models: provider === 'anthropic' ? ['claude-3-5-haiku'] : ['gpt-4o-mini'],
				}),
			});
		});

		// Hold the switch's own answer, so the Clear below is issued while that
		// commit is still unanswered and has to queue behind it.
		const switchSent = Promise.withResolvers<void>();
		const switchGate = Promise.withResolvers<void>();
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"provider":"anthropic"')) {
				await route.continue();
				return;
			}
			switchSent.resolve();
			await switchGate.promise;
			await route.continue();
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		const providerSelect = dialog.locator('select').first();
		await expect(providerSelect).toHaveValue('openai');
		await expect(dialog.locator('#llm-model')).toHaveValue('gpt-4o-mini');
		listed.length = 0;

		// The switch empties the dependent fields and sends the provider with
		// them. Clearing the stored key is a settings write of its own, issued
		// while the switch's answer is still in flight.
		await providerSelect.selectOption('anthropic');
		await switchSent.promise;
		await dialog.getByRole('button', { name: 'Clear' }).click();
		switchGate.resolve();

		// The switch was stored, so its models are listed even though the queued
		// clear had already taken the shared state back to `saving` by then.
		await expect(dialog.locator('#llm-model option[value="claude-3-5-haiku"]')).toHaveCount(1);
		expect(listed).toContain('anthropic');
		await expect(dialog.locator('#llm-model')).toHaveValue('');
		// The clear landed too: the field holds no key and the control for a
		// stored key is gone, so the switch is not reported as a rejection.
		await expect(dialog.getByLabel('API key')).toHaveValue('');
		await expect(dialog.getByRole('button', { name: 'Clear' })).toHaveCount(0);
	});

	test('given_a_rejected_switch_when_a_second_write_is_queued_then_the_unstored_provider_is_never_listed', async ({ page, request }) => {
		// A rejected selection is never listed, whatever the shared state holds
		// by the time its answer lands: the clear queued behind it moves that
		// state on, but the provider the server does not have must not be
		// queried. (The reject half of the shared-state regression is
		// unobservable here - a failed commit takes the rejection branch either
		// way - so this test's own guarantee is the unconditional one.)
		// No model is seeded: this test is about the listing decision alone, so
		// the model field stays out of it.
		const stored = await request.patch('/api/settings', {
			data: { ai: { provider: 'openai', apiKey: 'sk-stored' } },
		});
		expect(stored.ok()).toBe(true);

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
						{
							id: 'anthropic',
							name: 'Anthropic',
							envVar: 'ANTHROPIC_API_KEY',
							configured: true,
							supportsCustomEndpoint: false,
						},
					],
				}),
			});
		});

		const listed: string[] = [];
		await page.route('**/api/llm/models?*', async (route) => {
			const provider = new URL(route.request().url()).searchParams.get('provider') ?? '';
			listed.push(provider);
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					models: provider === 'anthropic' ? ['claude-3-5-haiku'] : ['gpt-4o-mini'],
				}),
			});
		});

		// The switch's answer is held, then rejected, so its own outcome is what
		// the callback has to read - not the state the queued clear left behind.
		let rejectSwitch = true;
		const switchSent = Promise.withResolvers<void>();
		const switchGate = Promise.withResolvers<void>();
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"provider":"anthropic"')) {
				await route.continue();
				return;
			}
			switchSent.resolve();
			await switchGate.promise;
			if (!rejectSwitch) {
				await route.continue();
				return;
			}
			await route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'provider unavailable', code: 'validation' }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		const providerSelect = dialog.locator('select').first();
		await expect(providerSelect).toHaveValue('openai');
		// The chip renders once this mount's read has answered; the Clear below
		// needs the read's key state, so waiting for the chip waits for it.
		await expect(dialog.locator('.llm-secret-state')).toHaveText('Stored in settings');
		listed.length = 0;

		await providerSelect.selectOption('anthropic');
		await switchSent.promise;
		await dialog.getByRole('button', { name: 'Clear' }).click();
		switchGate.resolve();

		// The queued clear lands after the rejection. Whatever the shared state
		// holds by then, the rejected selection must not be listed: the provider
		// it names is not the one the server has.
		await expect(dialog.locator('.llm-commit-state')).toHaveText('Saved');
		await page.waitForTimeout(300);
		expect(listed).not.toContain('anthropic');
		// The way back is the retry: it stores the selection and lists then.
		rejectSwitch = false;
		await providerSelect.press('Enter');
		await expect.poll(() => listed).toContain('anthropic');
		await expect(dialog.locator('#llm-model option[value="claude-3-5-haiku"]')).toHaveCount(1);
	});

	test('given_a_rejected_switch_when_a_later_write_is_answered_then_the_stored_model_is_not_filled', async ({ page, request }) => {
		// The switch was rejected, so the server still holds `openai` and its
		// model while the select shows `anthropic` with an emptied model field.
		// The answer of any later settings write reports the stored provider's
		// state, model included; filling that model here would put it into a
		// control that belongs to `anthropic` - a value the server does not hold
		// for the shown provider, which the next commit would store against the
		// stored one instead.
		const stored = await request.patch('/api/settings', {
			data: { ai: { provider: 'openai', model: 'gpt-4o-mini', apiKey: 'sk-stored' } },
		});
		expect(stored.ok()).toBe(true);

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
						{
							id: 'anthropic',
							name: 'Anthropic',
							envVar: 'ANTHROPIC_API_KEY',
							configured: true,
							supportsCustomEndpoint: false,
						},
					],
				}),
			});
		});

		await page.route('**/api/llm/models?*', async (route) => {
			const provider = new URL(route.request().url()).searchParams.get('provider') ?? '';
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({
					models: provider === 'anthropic' ? ['claude-3-5-haiku'] : ['gpt-4o-mini'],
				}),
			});
		});

		// The switch is held, then rejected: the stored provider and its model
		// survive on the server, while the select shows the rejected one.
		let rejectSwitch = true;
		const switchSent = Promise.withResolvers<void>();
		const switchGate = Promise.withResolvers<void>();
		await page.route('**/api/settings', async (route) => {
			const body = route.request().postData() ?? '';
			if (route.request().method() !== 'PATCH' || !body.includes('"provider":"anthropic"')) {
				await route.continue();
				return;
			}
			switchSent.resolve();
			await switchGate.promise;
			if (!rejectSwitch) {
				await route.continue();
				return;
			}
			await route.fulfill({
				status: 400,
				contentType: 'application/json',
				body: JSON.stringify({ error: 'provider unavailable', code: 'validation' }),
			});
		});

		await openLlmTab(page);
		const dialog = page.getByRole('dialog');
		const providerSelect = dialog.locator('select').first();
		await expect(providerSelect).toHaveValue('openai');
		await expect(dialog.locator('#llm-model')).toHaveValue('gpt-4o-mini');
		// The chip renders once this mount's read has answered; the Clear below
		// needs the read's key state, so waiting for the chip waits for it.
		await expect(dialog.locator('.llm-secret-state')).toHaveText('Stored in settings');

		// The switch empties the model field and is rejected.
		await providerSelect.selectOption('anthropic');
		await switchSent.promise;
		switchGate.resolve();
		await expect(dialog.locator('.form-error')).toBeVisible();
		await expect(providerSelect).toHaveValue('anthropic');
		await expect(dialog.locator('#llm-model')).toHaveValue('');

		// A later settings write - clearing the stored key - is answered with
		// the state the server still holds: `openai` and `gpt-4o-mini`. That
		// model must not be filled into `anthropic`'s emptied field.
		await dialog.getByRole('button', { name: 'Clear' }).click();
		await expect(dialog.locator('.llm-commit-state')).toHaveText('Saved');
		// The way the bug would surface: the retry stores `anthropic` and lists
		// it, and a stale model is not among its models, so the control would
		// degrade to the free-text fallback showing `openai`'s model.
		rejectSwitch = false;
		await providerSelect.press('Enter');
		await expect(dialog.locator('#llm-model option[value="claude-3-5-haiku"]')).toHaveCount(1);
		await expect(dialog.locator('#llm-model')).toHaveValue('');
		await expect(dialog.locator('.llm-error')).toHaveCount(0);
	});
});
