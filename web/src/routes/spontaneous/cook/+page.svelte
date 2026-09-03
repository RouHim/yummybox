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
		const raw = (() => {
			try {
				return sessionStorage.getItem('yummybox-cook-draft');
			} catch {
				return null;
			}
		})();
		if (!raw) return null;
		try {
			const parsed: unknown = JSON.parse(raw);
			if (
				typeof parsed !== 'object' ||
				parsed === null ||
				!Array.isArray((parsed as CookDraft).ingredients) ||
				typeof (parsed as CookDraft).name !== 'string' ||
				typeof (parsed as CookDraft).instructions !== 'string'
			) {
				return null;
			}
			return parsed as CookDraft;
		} catch {
			return null;
		}
	}

	// sessionStorage is browser-only: read it in an $effect (client-only) so
	// the initial render matches the server-rendered HTML and hydration never
	// diverges on a stored draft. `ready` gates the fallback so users don't
	// see a flash of "No draft to cook" on refresh/deep-link with a draft.
	let draft = $state<CookDraft | null>(null);
	let ready = $state(false);

	$effect(() => {
		draft = readDraft();
		ready = true;
	});

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
					source_url: null,
				}
			: null
	);
</script>

<main>
	{#if !ready}
		<!-- Draft is read client-side after mount; render nothing until then. -->
	{:else if meal}
		{#key meal.name}
			<CookingView {meal} imageUrl={draft?.imageDataUrl ?? null} />
		{/key}
	{:else}
		<p class="cooking-view__not-found">{t('cookDraftMissing')}</p>
		<a href="/spontaneous" class="btn btn--primary">{t('navGenerateMeal')}</a>
	{/if}
</main>
