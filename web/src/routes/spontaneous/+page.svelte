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
	let cookError = $state<string | null>(null);

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
		cookError = null;
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
			// A freshly generated draft supersedes any previously stored cook draft;
			// otherwise a direct visit to /spontaneous/cook would render stale data.
			sessionStorage.removeItem('yummybox-cook-draft');
			draftToken++;
			await tick();
			const prefersReduced = typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
			document.querySelector('.spontan-draft')?.scrollIntoView({ block: 'start', behavior: prefersReduced ? 'auto' : 'smooth' });
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
		cookError = null;
		sessionStorage.removeItem('yummybox-cook-draft');
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
			sessionStorage.removeItem('yummybox-cook-draft');
			await goto('/meals');
		} finally {
			saving = false;
		}
	}

	// Bound the draft image before storing it: generated JPEGs and user photos
	// routinely exceed the ~5 MB sessionStorage quota as base64 data URLs, so
	// re-encode to a downscaled JPEG that always fits.
	const COOK_DRAFT_MAX_EDGE = 1280;

	function downscaleImage(file: File): Promise<string> {
		return new Promise((resolve, reject) => {
			const url = URL.createObjectURL(file);
			const img = new Image();
			img.onload = () => {
				URL.revokeObjectURL(url);
				const scale = Math.min(
					1,
					COOK_DRAFT_MAX_EDGE / Math.max(img.naturalWidth, img.naturalHeight)
				);
				const canvas = document.createElement('canvas');
				canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
				canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
				const ctx = canvas.getContext('2d');
				if (!ctx) {
					reject(new Error('could not create canvas context'));
					return;
				}
				// Flatten transparency onto white so JPEG re-encoding keeps a clean background.
				ctx.fillStyle = '#fff';
				ctx.fillRect(0, 0, canvas.width, canvas.height);
				ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
				resolve(canvas.toDataURL('image/jpeg', 0.8));
			};
			img.onerror = () => {
				URL.revokeObjectURL(url);
				reject(new Error('could not load image'));
			};
			img.src = url;
		});
	}

	async function onCook(payload: MealFormPayload) {
		cookError = null;
		try {
			const imageDataUrl = payload.image ? await downscaleImage(payload.image) : null;
			sessionStorage.setItem(
				'yummybox-cook-draft',
				JSON.stringify({
					name: payload.name,
					ingredients: payload.ingredients,
					instructions: payload.instructions,
					portions: payload.portions,
					imageDataUrl,
				})
			);
		} catch {
			cookError = t('cookDraftSaveError');
			return;
		}
		await goto('/spontaneous/cook');
	}

	let hasInput = $derived(ingredients.trim().length > 0 || images.length > 0);
	let canGenerate = $derived(!!model.trim() && hasInput && !generating);
	let ingredientCount = $derived(ingredients.split('\n').filter((l: string) => l.trim().length > 0).length);
</script>

<main class="spontan-page">
	<header class="spontan-hero glass">
		<div class="spontan-hero__text">
			<h1 class="spontan-hero__title">{t('generatePageTitle')}</h1>
			<p class="spontan-hero__sub">{t('generateIntro')}</p>
		</div>
	</header>

	<section class="spontan-config glass" class:spontan-config--collapsed={settingsCollapsed && !!provider}>
		<div class="spontan-config__bar">
			<div class="spontan-config__label">
				<span class="spontan-config__icon" aria-hidden="true"><Icon name="layers" size={13} /></span>
				{#if provider && settingsCollapsed}
					<span class="spontan-config__summary">
						{t('llmProviderLabel')}: {providerName}, {t('llmModelLabel')}: {model}
					</span>
					<span class="spontan-config__ready" title={t('generateReady')} aria-label={t('generateReady')} role="status"></span>
				{:else}
					<span class="spontan-config__text">{t('generateSettingsLabel')}</span>
				{/if}
			</div>
			{#if provider}
				<button
					type="button"
					class="btn btn--ghost spontan-config__toggle"
					onclick={() => (settingsCollapsed = !settingsCollapsed)}
					disabled={generating}
					aria-expanded={!settingsCollapsed}
				>
					{settingsCollapsed ? t('llmSettingsChange') : t('llmSettingsHide')}
				</button>
			{/if}
		</div>

		{#if !provider}
			<p class="spontan-config__hint">{t('generateSettingsLabel')}</p>
		{/if}

		<div
			class="spontan-config__picker"
			class:spontan-config__picker--hidden={settingsCollapsed && !!provider}
		>
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
	</section>

	<section class="spontan-create">
		<div class="create-card">
			<div class="create-card__header">
				<div class="create-card__heading">
					<span class="create-card__icon" aria-hidden="true"><Icon name="utensils" size={15} /></span>
					<h2 class="create-card__title">{t('generateWhatTitle')}</h2>
				</div>
				<p class="create-card__hint">{t('generateWhatHint')}</p>
			</div>

			{#if providersReady}
				<label class="field create-card__field">
					<span class="field__label">{t('generateIngredientsLabel')}</span>
					<textarea
						bind:value={ingredients}
						placeholder={t('generateIngredientsPlaceholder')}
						rows="6"
						maxlength={20000}
						disabled={generating}
						class="create-card__textarea"
					></textarea>
					{#if ingredientCount > 0}
						<span class="field__helper create-card__helper">
							{ingredientCount === 1 ? t('ingredientCountOne') : t('ingredientCount', { count: String(ingredientCount) })}
						</span>
					{/if}
				</label>

				<div class="create-card__photos">
					<p class="create-card__photos-caption">{t('generateImagesHint')}</p>
					<GenerateImageInput
						bind:files={images}
						disabled={generating}
						onerror={(msg: string | null) => (generateError = msg)}
					/>
				</div>

				<div class="create-card__actions">
					<button
						type="button"
						class="btn btn--primary create-card__cta"
						onclick={onGenerate}
						aria-busy={generating}
						disabled={!canGenerate}
					>
						{#if generating}<Icon name="loader-circle" size={16} spin />{/if}
						{generating ? t('generateButtonLoading') : t('generateButton')}
					</button>
					<span class="create-card__or">{t('generateOrDropPhotos')}</span>
				</div>

				{#if generating}
					<p class="create-card__thinking" role="status" aria-live="polite">
						<span class="create-card__thinking-dot"></span>
						{t('generateThinking')}
					</p>
				{/if}
			{:else}
				<p class="create-card__empty">{t('generateConfigureProvider')}</p>
			{/if}

			{#if generateError}
				<p class="form-error create-card__error" role="alert">
					<Icon name="circle-alert" size={16} />
					<span>{generateError}</span>
				</p>
			{/if}
		</div>
	</section>

	{#if draft}
		<section class="spontan-draft generate-draft">
			<div class="spontan-draft__head glass">
				<div class="spontan-draft__badge">
					<span class="spontan-draft__dot" aria-hidden="true"></span>
					<span class="spontan-draft__badge-text">{t('generateDraftNotice')}</span>
				</div>
				<button type="button" class="btn btn--ghost spontan-draft__discard" onclick={discardDraft} disabled={saving}>
					<Icon name="x" size={14} />
					<span>{t('generateStartOver')}</span>
				</button>
			</div>
			<div class="spontan-draft__form">
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
			</div>
			{#if cookError}
				<p class="form-error spontan-draft__error" role="alert">
					<Icon name="circle-alert" size={16} />
					<span>{cookError}</span>
				</p>
			{/if}
		</section>
	{/if}
</main>

<style>
	.spontan-page {
		max-width: 880px;
		margin: 0 auto;
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
	}

	/* Hero: glass panel like other views so text is readable on ambient */
	.spontan-hero {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-5) var(--space-6);
		border-radius: var(--radius-lg);
	}

	.spontan-hero__title {
		margin: 0;
		font-family: var(--font-display);
		font-size: clamp(1.9rem, 4vw, 2.5rem);
		line-height: 1.05;
		letter-spacing: -0.02em;
		color: var(--color-text);
	}

	.spontan-hero__sub {
		margin: var(--space-1) 0 0;
		max-width: 42ch;
		font-size: var(--text-base);
		line-height: 1.6;
		color: var(--color-text-secondary);
	}

	/* Recessive AI config bar */
	.spontan-config {
		border-radius: var(--radius-lg);
		border: 1px solid var(--glass-border);
		padding: var(--space-3) var(--space-4);
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.spontan-config--collapsed {
		padding-block: var(--space-2);
	}

	.spontan-config__bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		min-height: 28px;
	}

	.spontan-config__label {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}

	.spontan-config__icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		border-radius: var(--radius-full);
		background: var(--color-surface-2);
		border: 1px solid var(--color-border);
		color: var(--color-text-muted);
		flex-shrink: 0;
	}

	.spontan-config__summary {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
		flex-wrap: wrap;
		font-size: var(--text-sm);
		color: var(--color-text);
		font-weight: var(--weight-medium);
	}

	.spontan-config__ready {
		width: 7px;
		height: 7px;
		border-radius: var(--radius-full);
		background: var(--color-success);
		box-shadow: 0 0 0 3px var(--color-success-bg);
		flex-shrink: 0;
	}

	.spontan-config__text {
		font-weight: var(--weight-medium);
		color: var(--color-text);
	}

	.spontan-config__hint {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-muted);
	}

	.spontan-config__toggle {
		flex-shrink: 0;
		padding: 6px 12px;
		font-size: var(--text-sm);
		border-radius: var(--radius-full);
	}

	.spontan-config__picker--hidden {
		display: none;
	}

	/* Primary create card: ingredients-first */
	.spontan-create {
		display: flex;
		flex-direction: column;
	}

	.create-card {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-top: 2px solid rgb(124 45 18 / 0.14);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-sm);
		padding: var(--space-6);
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
	}

	:root[data-theme='dark'] .create-card {
		border-top-color: rgb(217 119 87 / 0.18);
	}

	@media (prefers-color-scheme: dark) {
		:root:not([data-theme]) .create-card {
			border-top-color: rgb(217 119 87 / 0.18);
		}
	}

	.create-card__header {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-bottom: var(--space-4);
		border-bottom: 1px solid var(--color-border-light);
	}

	.create-card__heading {
		display: inline-flex;
		align-items: center;
		gap: var(--space-3);
	}

	.create-card__icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: var(--radius-full);
		background: var(--color-primary);
		color: var(--color-on-primary);
		flex-shrink: 0;
	}

	.create-card__title {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--weight-semibold);
		letter-spacing: -0.01em;
		color: var(--color-text);
		line-height: 1.2;
	}

	.create-card__hint {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-muted);
		line-height: 1.5;
	}

	.create-card__field :global(textarea) {
		min-height: 148px;
		resize: vertical;
		line-height: 1.55;
		padding-top: var(--space-3);
	}

	.create-card__textarea {
		font-size: var(--text-base);
	}

	.create-card__helper {
		display: block;
		margin-top: var(--space-2);
		font-size: var(--text-xs);
		color: var(--color-text-muted);
	}

	.create-card__photos {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-4);
		border-radius: var(--radius-md);
		background: var(--color-surface-2);
		border: 1px solid var(--color-border-light);
	}

	.create-card__photos-caption {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
		line-height: 1.5;
	}

	.create-card__actions {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		flex-wrap: wrap;
	}

	.create-card__cta {
		min-height: 44px;
		padding: 0 22px;
		border-radius: var(--radius-full);
		font-weight: var(--weight-semibold);
		font-size: var(--text-base);
		gap: var(--space-2);
		box-shadow: 0 4px 14px rgb(124 45 18 / 0.14);
	}

	:root[data-theme='dark'] .create-card__cta {
		box-shadow: 0 4px 14px rgb(0 0 0 / 0.22);
	}

	.create-card__or {
		font-size: var(--text-sm);
		color: var(--color-text-muted);
	}

	.create-card__thinking {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-muted);
	}

	.create-card__thinking-dot {
		width: 8px;
		height: 8px;
		border-radius: var(--radius-full);
		background: var(--color-primary);
		opacity: 0.9;
		animation: pulse-dot 1.4s ease-in-out infinite;
	}

	@keyframes pulse-dot {
		0%,
		100% {
			transform: scale(1);
			opacity: 0.9;
		}
		50% {
			transform: scale(0.85);
			opacity: 0.6;
		}
	}

	.create-card__error {
		margin: 0;
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	.create-card__empty {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-muted);
		padding: var(--space-3) 0;
	}

	/* Draft sheet */
	.spontan-draft {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		scroll-margin-top: calc(var(--app-bar-h) + var(--space-4));
		animation: draft-in 280ms ease-out;
	}

	@keyframes draft-in {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}

	.spontan-draft__head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		flex-wrap: wrap;
		padding: var(--space-3) var(--space-4);
		border-radius: var(--radius-lg);
		border: 1px solid var(--color-border);
	}

	.spontan-draft__badge {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		color: var(--color-success);
	}

	.spontan-draft__dot {
		width: 8px;
		height: 8px;
		border-radius: var(--radius-full);
		background: var(--color-success);
		box-shadow: 0 0 0 4px var(--color-success-bg);
		flex-shrink: 0;
	}

	.spontan-draft__badge-text {
		line-height: 1.3;
	}

	.spontan-draft__discard {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		border-radius: var(--radius-full);
		padding: 6px 12px;
		font-size: var(--text-sm);
	}

	.spontan-draft__form :global(.form-card) {
		margin: 0;
	}

	.spontan-draft__error {
		margin: 0;
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	@media (prefers-reduced-motion: reduce) {
		.spontan-draft {
			animation: none;
		}
		.create-card__thinking-dot {
			animation: none;
		}
	}

	@media (max-width: 759px) {
		.spontan-page {
			gap: var(--space-5);
		}
		.create-card {
			padding: var(--space-5);
		}
		.create-card__photos {
			padding: var(--space-3);
		}
		.spontan-config {
			padding: var(--space-3);
		}
		.create-card__actions {
			align-items: flex-start;
			flex-direction: column;
		}
		.create-card__or {
			display: none;
		}
	}
</style>
