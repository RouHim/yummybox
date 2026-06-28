<script lang="ts">
	import { listMeals, updateMeal, deleteMeal, mealImageUrl, createMeal, importFromUrl, importFromPaste, importFromLlm } from '$lib/api';
	import type { Meal, NewIngredientLine } from '$lib/types';
	import { t, formatDate } from '$lib/i18n';

	import { fly, fade, scale } from 'svelte/transition';
	import { flip } from 'svelte/animate';
	import { prefersReducedMotion, tierDuration, staggerDuration } from '$lib/motion';
	import Icon from '$lib/Icon.svelte';
	import DeleteConfirmDialog from '$lib/DeleteConfirmDialog.svelte';
	import MealForm from '$lib/MealForm.svelte';
	let meals = $state<Meal[]>([]);
	let searchTerm = $state('');
	let loadError = $state<string | null>(null);
	let reduced = $state(prefersReducedMotion());
	let deleteTarget = $state<Meal | null>(null);

	let editTarget = $state<Meal | null>(null);
	let editSubmitting = $state(false);

	let addOpen = $state(false);
	let formName = $state('');
	let formIngredients = $state<NewIngredientLine[]>([{ name: '', quantity: null }]);
	let formInstructions = $state('');
	let formImage = $state<File | null>(null);
	let removeImage = $state(false);
	let submitting = $state(false);
	let importMode = $state<'url' | 'paste' | 'llm'>('url');
	let importUrl = $state('');
	let importPaste = $state('');
	let importLlmModel = $state('');
	let importLlmHint = $state('');
	let importLlmImage = $state<File | null>(null);
	let importing = $state(false);
	let importError = $state<string | null>(null);
	let importToken = $state(0);

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
			importUrl = '';
			importPaste = '';
			importLlmModel = '';
			importLlmHint = '';
			importLlmImage = null;
			importToken++;
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

	async function onSubmitAdd(payload: {
		name: string; ingredients: NewIngredientLine[]; instructions: string;
		image: File | null; removeImage: boolean;
	}) {
		submitting = true;
		try {
			await createMeal(
				{ name: payload.name, ingredients: payload.ingredients, instructions: payload.instructions },
				payload.image
			);
			await loadMeals();
			closeAdd();
		} finally {
			submitting = false;
		}
	}

	function openAdd() {
		formName = ''; formIngredients = [{ name: '', quantity: null }]; formInstructions = '';
		formImage = null; removeImage = false; submitting = false;
		importMode = 'url'; importUrl = ''; importPaste = ''; importLlmModel = ''; importLlmHint = '';
		importLlmImage = null; importing = false; importError = null; importToken++;
		addOpen = true;
	}
	function closeAdd() { addOpen = false; }


	$effect(() => {
		const mq = window.matchMedia('(prefers-reduced-motion: reduce)');
		const handler = (e: MediaQueryListEvent) => (reduced = e.matches);
		mq.addEventListener('change', handler);
		return () => mq.removeEventListener('change', handler);
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

	function openEdit(meal: Meal) {
		editTarget = meal;
	}

	function closeEdit() {
		editTarget = null;
	}

	async function onSubmitEdit(payload: {
		name: string; ingredients: NewIngredientLine[]; instructions: string;
		image: File | null; removeImage: boolean;
	}) {
		// editTarget is non-null while the modal is open
		const id = editTarget!.id;
		editSubmitting = true;
		try {
			await updateMeal(id, { name: payload.name, ingredients: payload.ingredients, instructions: payload.instructions }, {
				image: payload.image,
				removeImage: payload.removeImage,
			});
			await loadMeals();
			closeEdit();
		} finally {
			editSubmitting = false;
		}
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

	// matches DeleteConfirmDialog.svelte focusTrap
	function focusTrap(node: HTMLElement) {
		const previouslyFocused = document.activeElement as HTMLElement | null;
		node.focus();
		function onKey(e: KeyboardEvent) {
			if (e.key !== 'Tab') return;
			const focusables = node.querySelectorAll<HTMLElement>(
				'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
			);
			if (focusables.length === 0) return;
			const first = focusables[0];
			const last = focusables[focusables.length - 1];
			if (e.shiftKey && document.activeElement === first) {
				e.preventDefault();
				last.focus();
			} else if (!e.shiftKey && document.activeElement === last) {
				e.preventDefault();
				first.focus();
			}
		}
		node.addEventListener('keydown', onKey);
		return {
			destroy() {
				node.removeEventListener('keydown', onKey);
				previouslyFocused?.focus?.();
			},
		};
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
			<button type="button" class="nav-link" onclick={openAdd}>
				<Icon name="plus" size={16} />
				{t('navAddMeal')}
			</button>
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
						class:meal-card--editing={editTarget?.id === meal.id}
						in:fly={{ y: -10, duration: tierDuration(250), delay: staggerDuration(i) }}
						out:scale={{ duration: tierDuration(200), start: 0.95 }}
						animate:flip={{ duration: tierDuration(200) }}
					>
						<a
							href="/meals/{meal.id}"
							class="meal-card"
							aria-label={t('mealCardCookAria', { name: meal.name })}
						>
							<div class="meal-card__media">
								{#if meal.has_image}
									<img
										src={mealImageUrl(meal.id)}
										alt={meal.name}
										class="meal-card__hero"
									/>
								{:else}
									<div class="meal-card__placeholder" aria-hidden="true">
										<Icon name="utensils" size={48} />
									</div>
								{/if}
								<div class="meal-card__overlay">
									<button
										type="button"
										class="btn btn--ghost meal-card__action-btn"
										aria-label={t('buttonEdit')}
										onclick={(e) => { e.preventDefault(); e.stopPropagation(); openEdit(meal); }}
									>
										<Icon name="pen-line" size={16} />
									</button>
									<button
										type="button"
										class="btn btn--danger-ghost meal-card__action-btn"
										aria-label={t('buttonDelete')}
										onclick={(e) => { e.preventDefault(); e.stopPropagation(); onDelete(meal); }}
									>
										<Icon name="trash-2" size={16} />
									</button>
								</div>
							</div>
							<div class="meal-card__body">
								<div class="meal-card__header">
									<h3 class="meal-card__name">{meal.name}</h3>
									<span class="meal-card__chip">
										{meal.ingredients.length === 1
											? t('ingredientCountOne')
											: t('ingredientCount', { count: String(meal.ingredients.length) })}
									</span>
								</div>
								<p class="meal-card__ingredients">{ingredientPreview(meal)}</p>
								<p class="meal-card__meta">
									<Icon name="calendar" size={14} />
									{#if meal.last_planned_at}
										{t('lastPlanned', { date: formatDate(meal.last_planned_at, { month: 'short', day: 'numeric' }) })}
									{:else}
										{t('lastPlannedNever')}
									{/if}
								</p>
							</div>
						</a>
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<DeleteConfirmDialog
		open={deleteTarget !== null}
		title={t('buttonDelete')}
		message={t('confirmDelete', { name: deleteTarget?.name ?? '' })}
		confirmLabel={t('buttonDelete')}
		cancelLabel={t('buttonCancel')}
		onconfirm={confirmDeleteDelete}
		oncancel={() => (deleteTarget = null)}
	/>
	{#if editTarget}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="edit-modal-overlay glass--strong" role="dialog" aria-label={t('formEditHeading', { name: editTarget.name || t('formUntitled') })} tabindex="-1" transition:fade={{ duration: tierDuration(200) }} onclick={closeEdit} onkeydown={(e) => { if (e.key === 'Escape') closeEdit(); }} use:focusTrap>
			<div class="edit-modal" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
				<MealForm
					editMode={true}
					editingMeal={editTarget}
					initialName={editTarget.name}
					initialIngredients={editTarget.ingredients.length > 0 ? editTarget.ingredients.map(i => ({ name: i.name, quantity: i.quantity })) : [{ name: '', quantity: null }]}
					initialInstructions={editTarget.instructions}
					submitting={editSubmitting}
					onsubmit={onSubmitEdit}
					oncancel={closeEdit}
				/>
			</div>
		</div>
	{/if}

	{#if addOpen}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="edit-modal-overlay glass--strong" role="dialog" aria-label={t('addMealTitle')} tabindex="-1" transition:fade={{ duration: tierDuration(200) }} onclick={closeAdd} onkeydown={(e) => { if (e.key === 'Escape') closeAdd(); }} use:focusTrap>
			<div class="edit-modal" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
				<button class="lightbox__close" onclick={closeAdd} aria-label={t('lightboxClose')}>
					<Icon name="x" size={24} />
				</button>
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
				{#key importToken}
					<MealForm
						editMode={false}
						initialName={formName}
						initialIngredients={formIngredients}
						initialInstructions={formInstructions}
						initialImage={formImage}
						submitting={submitting}
						onsubmit={onSubmitAdd}
					/>
				{/key}
			</div>
		</div>
	{/if}
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
	.meal-card {
		transition: transform var(--motion-morph), box-shadow var(--transition-base);
		will-change: transform;
	}
	.edit-modal-overlay {
		position: fixed;
		inset: 0;
		z-index: 1000;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--glass-scrim-dark);
		padding: var(--space-4);
	}
	.edit-modal {
		max-width: 640px;
		width: 90vw;
		max-height: 88vh;
		overflow-y: auto;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
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
</style>
