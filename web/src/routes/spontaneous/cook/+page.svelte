<script lang="ts">
	import CookingView from '$lib/components/CookingView.svelte';
	import { t } from '$lib/i18n';
	import type { Meal, NewIngredientLine } from '$lib/types';

	interface CookDraft {
		name: string;
		ingredients: NewIngredientLine[];
		instructions: string;
		portions: number | null;
		imageDataUrl: string | null;
	}

	function readDraft(): CookDraft | null {
		const raw = sessionStorage.getItem('yummybox-cook-draft');
		if (!raw) return null;
		try {
			return JSON.parse(raw) as CookDraft;
		} catch {
			return null;
		}
	}

	let draft = $state(readDraft());
	let meal = $derived<Meal | null>(
		draft
			? {
					id: 0,
					name: draft.name,
					ingredients: draft.ingredients.map((i) => ({ name: i.name, quantity: i.quantity })),
					instructions: draft.instructions,
					last_planned_at: null,
					created_at: '',
					updated_at: '',
					has_image: !!draft.imageDataUrl,
					portions: draft.portions,
				}
			: null
	);
</script>

<main>
	{#if meal}
		{#key meal.name}
			<CookingView {meal} imageUrl={draft?.imageDataUrl ?? null} />
		{/key}
	{:else}
		<p class="cooking-view__not-found">{t('cookDraftMissing')}</p>
		<a href="/spontaneous" class="btn btn--primary">{t('navGenerateMeal')}</a>
	{/if}
</main>
