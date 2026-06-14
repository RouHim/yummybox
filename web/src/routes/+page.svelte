<script lang="ts">
	import { listMeals, createMeal, updateMeal, deleteMeal } from '$lib/api';
	import { validateMeal } from '$lib/validation';
	import type { Meal, NewIngredientLine } from '$lib/types';
	import { t } from '$lib/i18n';

	import { fly, fade, scale } from 'svelte/transition';
	import { flip } from 'svelte/animate';
	import { transitionDuration, prefersReducedMotion } from '$lib/motion';
	import Icon from '$lib/Icon.svelte';

	let meals = $state<Meal[]>([]);
	let searchTerm = $state('');
	let editingId = $state<number | null>(null);
	let formName = $state('');
	let formIngredients = $state<NewIngredientLine[]>([{ name: '', quantity: null }]);
	let formError = $state<string | null>(null);
	let loadError = $state<string | null>(null);
	let submitting = $state(false);
	let reduced = $state(prefersReducedMotion());

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

	function validIngredientLines(): NewIngredientLine[] {
		return formIngredients.filter(r => r.name.trim().length > 0);
	}

	async function onSubmit() {
		formError = null;
		const valid = validIngredientLines();
		const result = validateMeal(formName, valid);
		if (!result.ok) {
			formError = t(result.messageKey);
			return;
		}
		submitting = true;
		try {
			if (editingId !== null) {
				await updateMeal(editingId, { name: formName.trim(), ingredients: valid });
			} else {
				await createMeal({ name: formName.trim(), ingredients: valid });
			}
			formName = '';
			formIngredients = [{ name: '', quantity: null }];
			editingId = null;
			formError = null;
			await loadMeals();
		} catch (err) {
			const raw = err instanceof Error ? err.message : '';
			formError = raw === '__REQUEST_FAILED__' ? t('errorSaveFailed') : raw;
		} finally {
			submitting = false;
		}
	}

	function startEdit(meal: Meal) {
		editingId = meal.id;
		formName = meal.name;
		formIngredients = meal.ingredients.length > 0
			? meal.ingredients.map(i => ({ name: i.name, quantity: i.quantity }))
			: [{ name: '', quantity: null }];
		formError = null;
	}

	function cancelEdit() {
		editingId = null;
		formName = '';
		formIngredients = [{ name: '', quantity: null }];
		formError = null;
	}

	function addIngredientRow() {
		formIngredients = [...formIngredients, { name: '', quantity: null }];
	}

	function removeIngredientRow(idx: number) {
		formIngredients = formIngredients.filter((_, i) => i !== idx);
	}

	async function onDelete(meal: Meal) {
		if (!confirm(t('confirmDelete', { name: meal.name }))) return;
		try {
			await deleteMeal(meal.id);
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
	<header class="page-header">
		<div>
			<h1>{t('appTitle')}</h1>
			<p class="page-header__subtitle">{t('appSubtitle')}</p>
		</div>
		<div class="page-header__right">
			<a href="/planner" class="nav-link">{t('navPlanner')}</a>
		</div>
	</header>

	<div class="search">
		<Icon name="search" class="search__icon" />
		<input
			type="search"
			class="search__input"
			bind:value={searchTerm}
			placeholder={t('searchPlaceholder')}
			aria-label={t('searchAriaLabel')}
		/>
	</div>

	{#if loadError}
		<p class="form-error" role="alert">
			<Icon name="alert" size={18} />
			<span>{loadError}</span>
		</p>
	{/if}

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
								<Icon name="trash" size={16} />
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
			{#if formError}
				<p class="form-error" role="alert">
					<Icon name="alert" size={18} />
					<span>{formError}</span>
				</p>
			{/if}
			<div class="form-card__actions">
				<button type="submit" class="btn btn--primary" disabled={submitting}>
					{#if editingId !== null}
						{t('buttonSave')}
					{:else}
						{t('buttonAdd')}
					{/if}
				</button>
				{#if editingId !== null}
					<button type="button" class="btn btn--ghost" onclick={cancelEdit}>{t('buttonCancel')}</button>
				{/if}
			</div>
		</form>
	</section>

	<section class="meal-list-section">
		<h2>{t('sectionAllMeals')}</h2>
		{#if meals.length === 0}
			{#if searchTerm}
				<div class="no-results">
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
				{#each meals as meal (meal.id)}
					<li
						class="meal-card"
						class:meal-card--editing={editingId === meal.id}
						in:fly={{ y: -10, duration: transitionDuration(200) }}
						out:scale={{ duration: transitionDuration(200), start: 0.95 }}
						animate:flip={{ duration: transitionDuration(200) }}
					>
						<h3 class="meal-card__name">{meal.name}</h3>
						<p class="meal-card__ingredients">
							{ingredientPreview(meal)}
						</p>
						<div class="meal-card__actions">
							<button type="button" class="btn btn--ghost" onclick={() => startEdit(meal)}>{t('buttonEdit')}</button>
							<button type="button" class="btn btn--danger-ghost" onclick={() => onDelete(meal)}>{t('buttonDelete')}</button>
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
</main>

<style>
	.page-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: var(--space-2);
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
	.nav-link {
		color: var(--color-primary);
		text-decoration: none;
		font-weight: var(--weight-medium);
		font-size: var(--text-sm);
	}
	.nav-link:hover {
		text-decoration: underline;
	}
	.form-card {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		padding: var(--space-6);
		box-shadow: var(--shadow-sm);
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
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
	@media (min-width: 768px) {
		.meal-list {
			display: grid;
			grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
			gap: var(--space-5);
		}
	}
</style>
