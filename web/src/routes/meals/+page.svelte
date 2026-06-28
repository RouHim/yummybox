<script lang="ts">
	import { listMeals, createMeal, updateMeal, deleteMeal, mealImageUrl, importFromUrl, importFromPaste, importFromLlm } from '$lib/api';
	import { validateMeal } from '$lib/validation';
	import type { Meal, NewIngredientLine } from '$lib/types';
	import { t } from '$lib/i18n';

	import { fly, fade, scale } from 'svelte/transition';
	import { flip } from 'svelte/animate';
	import { prefersReducedMotion, tierDuration, staggerDuration } from '$lib/motion';
	import Icon from '$lib/Icon.svelte';
	import DeleteConfirmDialog from '$lib/DeleteConfirmDialog.svelte';
	let meals = $state<Meal[]>([]);
	let searchTerm = $state('');
	let editingId = $state<number | null>(null);
	let formName = $state('');
	let formIngredients = $state<NewIngredientLine[]>([{ name: '', quantity: null }]);
	let formInstructions = $state('');
	let importMode = $state<'url' | 'paste' | 'llm'>('url');
	let importUrl = $state('');
	let importPaste = $state('');
	let importLlmModel = $state('');
	let importLlmHint = $state('');
	let importLlmImage = $state<File | null>(null);
	let importing = $state(false);
	let importError = $state<string | null>(null);
	let formError = $state<string | null>(null);
	let loadError = $state<string | null>(null);
	let submitting = $state(false);
	let formImage = $state<File | null>(null);
	let removeImage = $state(false);
	let lightboxSrc = $state<string | null>(null);
	let lightboxAlt = $state('');
	let reduced = $state(prefersReducedMotion());
	let lightboxEl = $state<HTMLDivElement | null>(null);
	let deleteTarget = $state<Meal | null>(null);
	$effect(() => {
		if (lightboxSrc && lightboxEl) lightboxEl.focus();
	});
	function openLightbox(meal: Meal) {
		lightboxSrc = mealImageUrl(meal.id);
		lightboxAlt = meal.name;
	}

	function closeLightbox() {
		lightboxSrc = null;
		lightboxAlt = '';
	}

	$effect(() => {
		const mq = window.matchMedia('(prefers-reduced-motion: reduce)');
		const handler = (e: MediaQueryListEvent) => (reduced = e.matches);
		mq.addEventListener('change', handler);
		return () => mq.removeEventListener('change', handler);
	});

	// Global Escape listener for lightbox
	$effect(() => {
		if (!lightboxSrc) return;
		function onKey(e: KeyboardEvent) {
			if (e.key === 'Escape') closeLightbox();
		}
		document.addEventListener('keydown', onKey);
		return () => document.removeEventListener('keydown', onKey);
	});

	async function loadMeals() {
		try {
			meals = await listMeals(searchTerm || undefined);
			loadError = null;
		} catch (err) {
			const raw = err instanceof Error ? err.message : '';
			loadError = raw === '__REQUEST_FAILED__' ? t('errorLoadFailed') : raw;
		}
	}

	$effect(() => {
		loadMeals();
	});

	$effect(() => {
		const term = searchTerm;
		const handle = setTimeout(() => {
			loadMeals();
		}, 150);
		return () => clearTimeout(handle);
	});

	function validIngredientLines(): NewIngredientLine[] {
		return formIngredients.filter(r => r.name.trim().length > 0);
	}

	async function onSubmit() {
		formError = null;
		const valid = validIngredientLines();
		const result = validateMeal(formName, valid, formInstructions);
		if (!result.ok) {
			formError = t(result.messageKey);
			return;
		}
		submitting = true;
		try {
			const id = editingId;
			if (id !== null) {
				await updateMeal(id, { name: formName.trim(), ingredients: valid, instructions: formInstructions.trim() }, {
					image: formImage,
					removeImage,
				});
			} else {
				await createMeal({ name: formName.trim(), ingredients: valid, instructions: formInstructions.trim() }, formImage);
			}
			formName = '';
			formIngredients = [{ name: '', quantity: null }];
			formInstructions = '';
			editingId = null;
			formImage = null;
			removeImage = false;
			formError = null;
			await loadMeals();
		} catch (err) {
			const raw = err instanceof Error ? err.message : String(err ?? '');
			formError = raw === '__REQUEST_FAILED__' ? t('errorSaveFailed') : raw;
		} finally {
			submitting = false;
		}
	}
	function startEdit(meal: Meal) {
		editingId = meal.id;
		formName = meal.name;
		formInstructions = meal.instructions;
		formIngredients = meal.ingredients.length > 0
			? meal.ingredients.map(i => ({ name: i.name, quantity: i.quantity }))
			: [{ name: '', quantity: null }];
		formImage = null;
		removeImage = false;
		formError = null;
	}

	function cancelEdit() {
		editingId = null;
		formName = '';
		formIngredients = [{ name: '', quantity: null }];
		formInstructions = '';
		formImage = null;
		removeImage = false;
		formError = null;
	}

	async function onImport() {
		importError = null;
		importing = true;
		try {
			const draft = importMode === 'url'
				? await importFromUrl(importUrl)
				: importMode === 'paste'
					? await importFromPaste(importPaste)
					: await importFromLlm(importLlmModel, importLlmHint || null, importLlmImage);
			formName = draft.name;
			formIngredients = draft.ingredients.length > 0
				? draft.ingredients.map(i => ({ name: i.name, quantity: i.quantity }))
				: [{ name: '', quantity: null }];
			formInstructions = draft.instructions;
			if (draft.imageBase64) {
				const bytes = Uint8Array.from(atob(draft.imageBase64), c => c.charCodeAt(0));
				formImage = new File([bytes], 'imported.jpg', { type: 'image/jpeg' });
				removeImage = false;
			}
			editingId = null;
			importUrl = '';
			importPaste = '';
			importLlmModel = '';
			importLlmHint = '';
			importLlmImage = null;
		} catch (err) {
			const raw = err instanceof Error ? err.message : '';
			importError = raw === '__REQUEST_FAILED__' ? t('importErrorFetch') : raw;
		} finally {
			importing = false;
		}
	}


	function onImportImageChange(e: Event) {
		const file = (e.target as HTMLInputElement).files?.[0] ?? null;
		importLlmImage = file;
	}

	function onImageChange(e: Event) {
		const file = (e.target as HTMLInputElement).files?.[0] ?? null;
		formImage = file;
		removeImage = false;
	}

	function onRemoveImageClick() {
		removeImage = true;
		formImage = null;
	}


	function onThumbnailKeydown(e: KeyboardEvent, meal: Meal) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			openLightbox(meal);
		}
	}
	function addIngredientRow() {
		formIngredients = [...formIngredients, { name: '', quantity: null }];
	}

	function removeIngredientRow(idx: number) {
		formIngredients = formIngredients.filter((_, i) => i !== idx);
	}

	async function onDelete(meal: Meal) {
		deleteTarget = meal;
	}

	async function confirmDeleteDelete() {
		if (!deleteTarget) return;
		const id = deleteTarget.id;
		deleteTarget = null;
		try {
			await deleteMeal(id);
			await loadMeals();
		} catch (err) {
			const raw = err instanceof Error ? err.message : '';
			loadError = raw === '__REQUEST_FAILED__' ? t('errorDeleteFailed') : raw;
		}
	}

	function ingredientPreview(meal: Meal): string {
		return meal.ingredients.map(i => i.quantity ? `${i.name} (${i.quantity})` : i.name).join(', ');
	}
</script>

<main>
	<header class="page-header glass">
		<div>
			<h1 class="page-header__wordmark">{t('appTitle')}</h1>
			<p class="page-header__subtitle">{t('appSubtitle')}</p>
		</div>
		<div class="page-header__right">
			<div class="page-header__search">
				<Icon name="search" class="search__icon" />
				<input
					type="search"
					class="search__input"
					bind:value={searchTerm}
					placeholder={t('searchPlaceholder')}
					aria-label={t('searchAriaLabel')}
				/>
			</div>
			<a href="/planner" class="nav-link">
				<Icon name="calendar" size={16} />
				{t('navPlanner')}
			</a>
		</div>
	</header>

	{#if loadError}
		<p class="form-error" role="alert">
			<Icon name="circle-alert" size={18} />
			<span>{loadError}</span>
		</p>
	{/if}

	<section class="import-card">
		<h2>{t('importHeading')}</h2>
		<div class="import-tabs">
			<button type="button" class="btn btn--ghost" class:btn--active={importMode === 'url'}
				onclick={() => importMode = 'url'}>
				{t('importTabUrl')}
			</button>
			<button type="button" class="btn btn--ghost" class:btn--active={importMode === 'paste'}
				onclick={() => importMode = 'paste'}>
				{t('importTabPaste')}
			</button>
			<button type="button" class="btn btn--ghost" class:btn--active={importMode === 'llm'}
				onclick={() => importMode = 'llm'}>
				{t('importTabLlm')}
			</button>
		</div>
		{#if importMode === 'url'}
			<input type="url" bind:value={importUrl} placeholder={t('importUrlPlaceholder')} />
			<button type="button" class="btn btn--primary" onclick={onImport} disabled={importing || !importUrl.trim()}>
				{t('importButtonFetch')}
			</button>
		{:else if importMode === 'paste'}
			<textarea bind:value={importPaste} placeholder={t('importPastePlaceholder')} rows="6"></textarea>
			<button type="button" class="btn btn--primary" onclick={onImport} disabled={importing || !importPaste.trim()}>
				{t('importButtonPaste')}
			</button>
		{:else}
			<p class="import-info">{t('importLlmInfo')}</p>
			<input type="text" bind:value={importLlmModel} placeholder={t('importLlmModelPlaceholder')} />
			<textarea bind:value={importLlmHint} placeholder={t('importLlmHintPlaceholder')} rows="3"></textarea>
			<input type="file" accept="image/*" onchange={onImportImageChange} />
			<button type="button" class="btn btn--primary" onclick={onImport}
				disabled={importing || !importLlmModel.trim() || (!importLlmHint.trim() && !importLlmImage)}>
				{t('importButtonLlm')}
			</button>
		{/if}
		{#if importError}
			<p class="form-error" role="alert">
				<Icon name="circle-alert" size={18} />
				<span>{importError}</span>
			</p>
		{/if}
	</section>

	<section class="form-card">
		<h2>
			{#if editingId !== null}
				{t('formEditHeading', { name: formName || t('formUntitled') })}
			{:else}
				{t('formAddHeading')}
			{/if}
		</h2>
		<form onsubmit={(e) => { e.preventDefault(); onSubmit(); }} class="form-card__form">
			<label class="field">
				<span class="field__label">{t('fieldNameLabel')}</span>
				<input
					type="text"
					bind:value={formName}
					placeholder={t('fieldNamePlaceholder')}
					maxlength={200}
				/>
			</label>
			<fieldset class="field">
				<legend class="field__label">{t('fieldIngredientsLabel')}</legend>
				<div class="ingredient-rows">
					{#each formIngredients as ing, i (i)}
						<div class="ingredient-row">
							<input
								type="text"
								bind:value={ing.name}
								placeholder={t('fieldIngredientName')}
								maxlength={100}
								aria-label="{t('fieldIngredientName')} {i + 1}"
							/>
							<input
								type="text"
								value={ing.quantity ?? ''}
								oninput={(e) => { ing.quantity = (e.target as HTMLInputElement).value || null; }}
								placeholder={t('fieldIngredientQuantity')}
								maxlength={50}
								class="ingredient-row__quantity"
							/>
							<button type="button" class="btn btn--ghost btn--icon"
								onclick={() => removeIngredientRow(i)}
								aria-label={t('fieldIngredientRemove')}
								disabled={formIngredients.length <= 1}
							>
								<Icon name="trash-2" size={16} />
							</button>
						</div>
					{/each}
				</div>
				<button type="button" class="btn btn--ghost"
					onclick={addIngredientRow}
					disabled={formIngredients.length >= 100}
				>
					<Icon name="plus" size={14} /> {t('fieldIngredientAdd')}
				</button>
			</fieldset>

			<label class="field">
				<span class="field__label">{t('fieldInstructionsLabel')}</span>
				<textarea
					bind:value={formInstructions}
					placeholder={t('fieldInstructionsPlaceholder')}
					maxlength={20000}
					rows="6"
				></textarea>
			</label>

			<!-- Image field -->
			<div class="field">
				<span class="field__label">{t('fieldImageLabel')}</span>
				{#if editingId !== null}
					{@const currentMeal = meals.find(m => m.id === editingId)}
					{#if currentMeal?.has_image && !removeImage}
						<div class="meal-image-controls">
							<img src={mealImageUrl(editingId)} alt="" class="meal-image-preview" />
							<button type="button" class="btn btn--ghost" onclick={onRemoveImageClick}>
								<Icon name="trash-2" size={14} /> {t('fieldImageRemove')}
							</button>
						</div>
					{/if}
				{/if}
				<input
					type="file"
					accept="image/*"
					onchange={onImageChange}
				/>
				{#if formImage}
					<span class="image-status">{t('imageStaged')}</span>
				{:else if removeImage}
					<span class="image-status">{t('imageStagedRemove')}</span>
				{/if}
			</div>
			{#if formError}
				<p class="form-error" role="alert">
					<Icon name="circle-alert" size={18} />
					<span>{formError}</span>
				</p>
			{/if}
			<div class="form-card__actions">
				<button type="submit" class="btn btn--primary" disabled={submitting}>
					{#if editingId !== null}
						<Icon name="check" size={16} />
						{t('buttonSave')}
					{:else}
						<Icon name="plus" size={16} />
						{t('buttonAdd')}
					{/if}
				</button>
				{#if editingId !== null}
					<button type="button" class="btn btn--ghost" onclick={cancelEdit}>
						<Icon name="x" size={16} />
						{t('buttonCancel')}
					</button>
				{/if}
			</div>
		</form>
	</section>

	<section class="meal-list-section">
		<h2>{t('sectionAllMeals')}</h2>
		{#if meals.length === 0}
			{#if searchTerm}
			<div class="no-results">
				<Icon name="search" size={32} class="no-results__icon" />
				<p>{t('noResults', { search: searchTerm })}</p>
			</div>
			{:else}
				<div class="empty-state" data-testid="empty-state">
					<Icon name="empty-meals" size={96} class="empty-state__icon" />
					<h3 class="empty-state__title">{t('emptyStateTitle')}</h3>
					<p>{t('emptyStateDescription')}</p>
				</div>
			{/if}
		{:else}
			<ul class="meal-list">
				{#each meals as meal, i (meal.id)}
					<li
						class="meal-card"
						class:meal-card--editing={editingId === meal.id}
						in:fly={{ y: -10, duration: tierDuration(250), delay: staggerDuration(i) }}
						out:scale={{ duration: tierDuration(200), start: 0.95 }}
						animate:flip={{ duration: tierDuration(200) }}
					>
						{#if meal.has_image}
							<img
								src={mealImageUrl(meal.id)}
								alt=""
								class="meal-card__thumb"
								role="button"
								tabindex="0"
								onclick={() => openLightbox(meal)}
								onkeydown={(e) => onThumbnailKeydown(e, meal)}
							/>
						{/if}
						<h3 class="meal-card__name">{meal.name}</h3>
						<p class="meal-card__ingredients">
							{ingredientPreview(meal)}
						</p>
						<div class="meal-card__actions">
							<button type="button" class="btn btn--ghost" onclick={() => startEdit(meal)}>
								<Icon name="pen-line" size={16} />
								{t('buttonEdit')}
							</button>
							<button type="button" class="btn btn--danger-ghost" onclick={() => onDelete(meal)}>
								<Icon name="trash-2" size={16} />
								{t('buttonDelete')}
							</button>
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	{#if lightboxSrc}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="lightbox glass--strong"
			role="dialog"
			aria-label={lightboxAlt}
			tabindex="-1"
			bind:this={lightboxEl}
			transition:fade={{ duration: tierDuration(200) }}
			onclick={closeLightbox}
			onkeydown={(e) => { if (e.key === 'Escape') closeLightbox(); }}
		>
			<button class="lightbox__close" onclick={closeLightbox} aria-label={t('lightboxClose')}>
				<Icon name="x" size={24} />
			</button>
			<img
				src={lightboxSrc}
				alt={lightboxAlt}
				in:scale={{ duration: tierDuration(250), start: 0.92 }}
				onclick={(e) => e.stopPropagation()}
				onkeydown={() => {}}
			/>
		</div>
	{/if}
	<DeleteConfirmDialog
		open={deleteTarget !== null}
		title={t('buttonDelete')}
		message={t('confirmDelete', { name: deleteTarget?.name ?? '' })}
		confirmLabel={t('buttonDelete')}
		cancelLabel={t('buttonCancel')}
		onconfirm={confirmDeleteDelete}
		oncancel={() => (deleteTarget = null)}
	/>
</main>

<style>
	.page-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--space-3);
		margin-bottom: var(--space-2);
		position: sticky;
		top: 0;
		z-index: 20;
		padding: var(--space-3) var(--space-4);
		border-radius: 0;
		border-left: none;
		border-right: none;
		border-top: none;
	}
	.page-header__wordmark {
		font-family: var(--font-display);
		font-size: var(--text-2xl);
	}
	.page-header__subtitle {
		margin: 0;
		color: var(--color-text-secondary);
		font-size: var(--text-base);
	}
	.page-header__right {
		display: flex;
		align-items: center;
		gap: var(--space-3);
	}
	.page-header__search {
		position: relative;
		display: flex;
		align-items: center;
	}
	.page-header__search .search__input {
		background: transparent;
		border: 1px solid var(--glass-border-inner);
		border-radius: var(--radius-full);
		padding: var(--space-1) var(--space-3) var(--space-1) var(--space-8);
		font-size: var(--text-sm);
		color: var(--color-text);
		width: 220px;
	}
	.page-header__search .search__input:focus {
		outline: none;
		border-color: var(--color-primary);
		box-shadow: none;
	}
	.nav-link {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		color: var(--color-primary);
		text-decoration: none;
		font-weight: var(--weight-medium);
		font-size: var(--text-sm);
	}
	.nav-link:hover { text-decoration: underline; }
	.page-header.glass .nav-link {
		background: var(--glass-scrim);
		border-radius: var(--radius-full);
		padding: var(--space-1) var(--space-2);
	}
	.form-card {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-top: 2px solid rgb(124 45 18 / 0.15);
		border-radius: var(--radius-lg);
		padding: var(--space-6);
		box-shadow: var(--shadow-sm);
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.form-card h2 {
		font-family: var(--font-display);
	}
	.form-card__form {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	fieldset.field {
		border: none;
		padding: 0;
		margin: 0;
	}
	.ingredient-rows {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.ingredient-row {
		display: flex;
		gap: var(--space-2);
		align-items: center;
	}
	.ingredient-row input {
		flex: 1;
		min-width: 0;
	}
	.ingredient-row__quantity {
		max-width: 140px;
	}
	.btn--icon {
		padding: var(--space-1);
		min-width: 36px;
		min-height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.form-card__actions {
		display: flex;
		gap: var(--space-2);
		flex-wrap: wrap;
	}
	.meal-list-section h2 {
		margin-bottom: var(--space-4);
	}
	.meal-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.empty-state__title {
		margin: 0;
		font-size: var(--text-xl);
		font-weight: var(--weight-semibold);
		color: var(--color-text);
	}
	.empty-state p {
		margin: 0;
		color: var(--color-text-secondary);
	}

	.no-results__icon {
		display: block;
		margin: 0 auto var(--space-3);
		color: var(--color-text-muted);
	}
	@media (min-width: 768px) {
		.meal-list {
			display: grid;
			grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
			gap: var(--space-5);
		}
	}

	/* Image */
	.meal-image-controls {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-2);
	}
	.meal-image-preview {
		max-width: 100px;
		max-height: 100px;
		object-fit: cover;
		border-radius: var(--radius-sm);
	}
	.image-status {
		display: block;
		margin-top: var(--space-1);
		font-size: var(--text-xs);
		color: var(--color-text-secondary);
	}
	.meal-card__thumb {
		max-width: 100px;
		max-height: 100px;
		object-fit: cover;
		border-radius: var(--radius-sm);
		cursor: pointer;
		display: block;
	}
	.lightbox {
		position: fixed;
		inset: 0;
		background: var(--glass-scrim-dark);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}
	.lightbox img {
		max-width: 90vw;
		max-height: 90vh;
		object-fit: contain;
		border: 0;
	}
	.lightbox__close {
		position: absolute;
		top: var(--space-4);
		right: var(--space-4);
		color: white;
		background: transparent;
		border: 0;
		cursor: pointer;
		padding: var(--space-2);
	}

	/* Import card — recessive, secondary to authoring form */
	.import-card {
		background: var(--color-surface-2);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		padding: var(--space-6);
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		margin-bottom: var(--space-5);
	}
	.import-card h2 {
		margin: 0;
	}
	.import-tabs {
		display: flex;
		gap: var(--space-2);
	}
	.import-tabs .btn {
		transition: border-color var(--motion-morph), color var(--motion-morph), background var(--motion-morph);
	}
	.btn--active {
		border-color: var(--color-primary);
		color: var(--color-primary);
		font-weight: var(--weight-medium);
	}
	.meal-card {
		transition: transform var(--motion-morph), box-shadow var(--transition-base);
		will-change: transform;
	}
	@keyframes shake {
		0%, 100% { transform: translateX(0); }
		20% { transform: translateX(-6px); }
		40% { transform: translateX(6px); }
		60% { transform: translateX(-4px); }
		80% { transform: translateX(4px); }
	}
	.form-error {
		animation: shake var(--motion-exit);
	}
</style>
