<script lang="ts">
	import { listMeals, createMeal, generateMeal } from '$lib/api';
	import type { Meal, MealFormPayload, NewIngredientLine } from '$lib/types';
	import { persistLlmConfig } from '$lib/llm-config.svelte';
	import { llmErrorMessage } from '$lib/llm-error';
	import { t } from '$lib/i18n';
	import { goto } from '$app/navigation';
	import { tick } from 'svelte';
	import Icon from '$lib/Icon.svelte';
	import MealForm from '$lib/MealForm.svelte';
	import LlmConfigPicker from '$lib/components/LlmConfigPicker.svelte';
	import GenerateImageInput from '$lib/components/GenerateImageInput.svelte';

	let meals = $state<Meal[]>([]);
	let existingMealNames = $derived(
		new Set(meals.map((m) => m.name.trim().toLowerCase().split(/\s+/).join(' ')))
	);

	let provider = $state('');
	let providerName = $state('');
	let model = $state('');
	let customBaseUrl = $state('');
	let customApiKey = $state('');
	let providersReady = $state(true);
	let settingsCollapsed = $state(false);

	let ingredients = $state('');
	let images = $state<File[]>([]);
	let generating = $state(false);
	let generateError = $state<string | null>(null);

	let draft = $state<{
		name: string;
		ingredients: NewIngredientLine[];
		instructions: string;
		portions: number | null;
	} | null>(null);
	let draftImage = $state<File | null>(null);
	let draftToken = $state(0);
	let saving = $state(false);

	async function loadMeals() {
		try {
			meals = await listMeals();
		} catch {
			// Duplicate-name warnings are best-effort; the backend re-validates.
		}
	}
	loadMeals();

	async function onGenerate() {
		generateError = null;
		generating = true;
		try {
			const d = await generateMeal(
				model,
				ingredients,
				images,
				provider === 'custom' ? customBaseUrl : undefined,
				provider === 'custom' ? customApiKey : undefined,
			);
			draft = {
				name: d.name,
				ingredients: d.ingredients.length > 0
					? d.ingredients.map((i) => ({ name: i.name, quantity: i.quantity }))
					: [{ name: '', quantity: null }],
				instructions: d.instructions,
				portions: d.portions,
			};
			if (d.imageBase64) {
				const bytes = Uint8Array.from(atob(d.imageBase64), (c) => c.charCodeAt(0));
				draftImage = new File([bytes], 'generated.jpg', { type: 'image/jpeg' });
			} else {
				draftImage = null;
			}
			persistLlmConfig({ provider, model, customBaseUrl, customApiKey });
			settingsCollapsed = true;
			draftToken++;
			await tick();
			document.querySelector('.generate-draft')?.scrollIntoView({ block: 'start' });
		} catch (err) {
			generateError = llmErrorMessage(err);
		} finally {
			generating = false;
		}
	}

	function discardDraft() {
		draft = null;
		draftImage = null;
		draftToken++;
	}

	async function onSave(payload: MealFormPayload) {
		saving = true;
		try {
			await createMeal(
				{
					name: payload.name,
					ingredients: payload.ingredients,
					instructions: payload.instructions,
					portions: payload.portions,
				},
				payload.image,
			);
			await goto('/meals');
		} finally {
			saving = false;
		}
	}

	function fileToDataUrl(file: File): Promise<string> {
		return new Promise((resolve, reject) => {
			const reader = new FileReader();
			reader.onload = () => resolve(String(reader.result));
			reader.onerror = () => reject(reader.error);
			reader.readAsDataURL(file);
		});
	}

	async function onCook(payload: MealFormPayload) {
		const imageDataUrl = payload.image ? await fileToDataUrl(payload.image) : null;
		sessionStorage.setItem('yummybox-cook-draft', JSON.stringify({
			name: payload.name,
			ingredients: payload.ingredients,
			instructions: payload.instructions,
			portions: payload.portions,
			imageDataUrl,
		}));
		await goto('/spontaneous/cook');
	}
</script>

<main>
	<h1 class="spontan-heading glass">{t('generatePageTitle')}</h1>

	<div class="spontan-grid">
		<div class="spontan-main">
			<section class="generate-card">
				<p class="generate-intro">
					<span class="generate-intro__icon"><Icon name="sparkles" size={14} /></span>
					<span>{t('generateIntro')}</span>
				</p>

				{#if providersReady}
					<label class="import-field">
						<span>{t('generateIngredientsLabel')}</span>
						<textarea
							bind:value={ingredients}
							placeholder={t('generateIngredientsPlaceholder')}
							rows="8"
							maxlength={20000}
							disabled={generating}
						></textarea>
					</label>

					<div class="generate-photos">
						<p class="import-info">{t('generateImagesHint')}</p>
						<GenerateImageInput
							bind:files={images}
							disabled={generating}
							onerror={(msg) => (generateError = msg)}
						/>
					</div>

					<div class="generate-actions">
						<button type="button" class="btn btn--primary" onclick={onGenerate} aria-busy={generating}
							disabled={generating || !model.trim() || (!ingredients.trim() && images.length === 0)}>
							{#if generating}<Icon name="loader-circle" size={16} spin />{/if}
							{generating ? t('generateButtonLoading') : t('generateButton')}
						</button>
						{#if generating}
							<p class="generate-thinking">{t('generateThinking')}</p>
						{/if}
					</div>
				{/if}

				{#if generateError}
					<p class="form-error" role="alert">
						<Icon name="circle-alert" size={18} />
						<span>{generateError}</span>
					</p>
				{/if}
			</section>

			{#if draft}
				<section class="generate-draft">
					<div class="generate-draft__header">
						<p class="generate-draft-notice">
							<span class="generate-draft-notice__icon"><Icon name="check" size={14} /></span>
							{t('generateDraftNotice')}
						</p>
						<button type="button" class="btn btn--ghost" onclick={discardDraft} disabled={saving}>
							{t('generateStartOver')}
						</button>
					</div>
					{#key draftToken}
						<MealForm
							editMode={false}
							initialName={draft.name}
							initialIngredients={draft.ingredients}
							initialInstructions={draft.instructions}
							initialPortions={draft.portions}
							initialImage={draftImage}
							submitting={saving}
							existingNames={existingMealNames}
							onsubmit={onSave}
							submitLabel={t('buttonSave')}
							oncook={onCook}
						/>
					{/key}
				</section>
			{/if}
		</div>

		<aside class="spontan-settings">
			{#if provider}
				<div class="generate-settings-toggle">
					{#if settingsCollapsed}
						<span class="generate-settings-summary">
							{t('llmProviderLabel')}: {providerName}, {t('llmModelLabel')}: {model}
						</span>
					{:else}
						<span class="generate-settings-summary">{t('generateSettingsLabel')}</span>
					{/if}
					<button type="button" class="btn btn--ghost" onclick={() => (settingsCollapsed = !settingsCollapsed)} disabled={generating}>
						{settingsCollapsed ? t('llmSettingsChange') : t('llmSettingsHide')}
					</button>
				</div>
			{:else}
				<p class="generate-settings-summary">{t('generateSettingsLabel')}</p>
			{/if}
			<div class="generate-picker-host" class:generate-picker-hidden={settingsCollapsed && provider}>
				<LlmConfigPicker
					bind:provider
					bind:providerName
					bind:model
					bind:customBaseUrl
					bind:customApiKey
					bind:providersReady
					disabled={generating}
					onrestored={() => {
						if (provider && model) settingsCollapsed = true;
					}}
				/>
			</div>
		</aside>
	</div>
</main>

<style>
	main {
		max-width: 960px;
	}

	.spontan-grid {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
		align-items: stretch;
	}

	/* Mobile: AI settings come first, they are the prerequisite for generating. */
	.spontan-settings {
		order: -1;
	}

	.spontan-main {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
		min-width: 0;
	}
	main {
		max-width: 960px;
	}

	.spontan-heading {
		display: inline-block;
		padding: var(--space-2) var(--space-4);
		border-radius: var(--radius-lg);
		margin-bottom: var(--space-4);
	}
	.generate-card {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		background: var(--color-surface-2);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		padding: var(--space-5);
	}

	.generate-intro {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
	.generate-intro__icon {
		flex-shrink: 0;
		display: flex;
		color: var(--color-primary);
	}

	.generate-photos {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.generate-actions {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-2);
	}
	.generate-thinking {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-muted);
	}

	.generate-draft {
		scroll-margin-top: calc(var(--app-bar-h) + var(--space-4));
	}
	.generate-draft__header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		margin-bottom: var(--space-4);
		flex-wrap: wrap;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: var(--space-2) var(--space-3);
	}
	.generate-draft-notice {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-success);
	}
	.generate-draft-notice__icon {
		flex-shrink: 0;
		display: flex;
	}

	.spontan-settings {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		padding: var(--space-4);
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.generate-settings-toggle {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		flex-wrap: wrap;
	}
	.generate-settings-summary {
		margin: 0;
		color: var(--color-text-secondary);
		font-size: var(--text-sm);
		text-wrap: pretty;
	}
	.generate-picker-host.generate-picker-hidden {
		display: none;
	}

	@media (min-width: 768px) {
		.spontan-grid {
			display: grid;
			grid-template-columns: minmax(0, 1fr) 300px;
			align-items: start;
		}
		.spontan-settings {
			order: 0;
			position: sticky;
			top: calc(var(--app-bar-h) + var(--space-4));
		}
	}
</style>
