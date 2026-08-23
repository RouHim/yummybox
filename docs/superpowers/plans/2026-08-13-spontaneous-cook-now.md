# Spontaneous Cooking: Save Label + Cook-Now-Without-Persisting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the Spontaneous cooking view, rename the draft submit button to "Save" and add a "Cook now" action that opens the cooking view for the edited draft without writing anything to the database (session-scoped, forgotten when the tab closes).

**Architecture:** No backend changes. The cooking presentation (hero, header, portions stepper, quantity scaling, ingredients, instructions) is extracted from `web/src/routes/meals/[id]/+page.svelte` into a shared `CookingView` component used by both the meal-detail page and a new `/spontaneous/cook` route. The draft crosses the page boundary via `sessionStorage` under one key (same spirit as the existing localStorage LLM config; no state store). MealForm gains two narrow props: a submit-label override and an optional validated "cook" callback wired through the same validation path as submit.

**Tech Stack:** Svelte 5 (runes, snippets), SvelteKit (adapter-static, SPA fallback), TypeScript strict, Playwright (workflow suite in `tests/`), Vitest.

## Global Constraints

- No new dependencies; no backend/API changes; no DB writes for the cook path.
- Existing project rules: flat Svelte component usage, BDD test names, i18n keys in `en.ts`/`de.ts`/`types.ts` union (dictionary parity test enforces both languages), e2e workflow specs run against the release binary via `cd tests && npm test`.
- Keep every existing `.cooking-view__*` class name and DOM structure identical when extracting `CookingView` — `tests/e2e/cooking-view.spec.ts`, `portions.spec.ts`, `edit-meal-detail.spec.ts`, and `edit-meal-full.spec.ts` are the regression gate for the extraction.
- The add-meal and edit-meal dialogs keep their existing submit label ("Add" / "Hinzufügen"); only the spontaneous draft form shows "Save".
- New visible strings: no em-dashes; en + de translations required.
- The `/spontaneous` route stays; the cook route is `/spontaneous/cook`.
- `sessionStorage` key for the cook draft: `yummybox-cook-draft` (JSON, see Task 3).

---

### Task 1: MealForm submit-label override and "Cook now" action slot

**Files:**
- Modify: `web/src/lib/MealForm.svelte` (props at lines 9-40, `onSubmit` at lines 87-117, actions row at lines 221-237)
- Modify: `tests/e2e/generate-meal.spec.ts` (draft save clicks at lines 59-60 and 93)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `MealForm` new props: `submitLabel?: string` (used when `editMode === false`; falls back to `t('buttonAdd')`) and `oncook?: (payload: MealFormPayload) => void | Promise<void>` where `MealFormPayload = { name: string; ingredients: NewIngredientLine[]; instructions: string; portions: number | null; image: File | null; removeImage: boolean }` (identical to the existing `onsubmit` payload shape).
  - `MealForm` renders a ghost "Cook now" button before the submit button when `oncook` is provided. The button label uses the new key `cookNowButton`.
  - Internal helper `buildPayload(): MealFormPayload | null` extracted from the current `onSubmit` validation flow (validates meal + image, sets `formError` on failure, returns null); `onSubmit` and the new `onCookClick` both call it.

- [ ] **Step 1: Update the e2e save-button contract to expect "Save"**

In `tests/e2e/generate-meal.spec.ts`, replace both draft-save clicks (currently `getByRole('button', { name: /^(Add|Hinzufügen)$/ })`) with the exact name:

```ts
await page.getByRole('button', { name: /^(Save|Speichern)$/ }).click();
```

(One occurrence in test `'generates a recipe via AI and saves it as a meal'`, one in `'restores the provider config and collapses AI settings on revisit'`.)

- [ ] **Step 2: Run the affected spec to verify it fails**

Run: `cd tests && npx playwright test e2e/generate-meal.spec.ts -g "saves it as a meal"`
Expected: FAIL — the draft form still shows the "Add" button, so the `Save` locator times out.

- [ ] **Step 3: Add i18n keys**

`web/src/lib/i18n/en.ts` (insert after `generateStartOver`, line 109):

```ts
	cookNowButton: 'Cook now',
	cookDraftMissing: 'No draft to cook. Create one first.',
	cookDraftSaveError: 'The draft could not be saved. Try again with a smaller image.',
```

`web/src/lib/i18n/de.ts` (same position, line 109):

```ts
	cookNowButton: 'Jetzt kochen',
	cookDraftMissing: 'Kein Entwurf zum Kochen. Erstelle zuerst einen.',
	cookDraftSaveError: 'Der Entwurf konnte nicht gespeichert werden. Versuche es erneut mit einem kleineren Bild.',
```

`web/src/lib/i18n/types.ts` (insert after `generateStartOver`, line 115):

```ts
	| 'cookNowButton'
	| 'cookDraftMissing'
	| 'cookDraftSaveError'
```

- [ ] **Step 4: Run the i18n parity test**

Run: `cd web && npm test -- --run i18n`
Expected: PASS (parity is per-key, new keys exist in both dictionaries).

- [ ] **Step 5: Implement the MealForm props and payload extraction**

`web/src/lib/MealForm.svelte`:

a) Extend props (lines 9-40): add to the destructure and its type:

```ts
	submitLabel,
	oncook,
```

```ts
	submitLabel?: string;
	oncook?: (payload: MealFormPayload) => void | Promise<void>;
```

Add above the component or in `$lib/types.ts` (plan choice: put it in `web/src/lib/types.ts` next to `MealPayload` to keep the form contract discoverable):

```ts
export interface MealFormPayload {
	name: string;
	ingredients: NewIngredientLine[];
	instructions: string;
	portions: number | null;
	image: File | null;
	removeImage: boolean;
}
```

b) Refactor `onSubmit` (lines 87-117): extract the validation body into:

```ts
	function buildPayload(): MealFormPayload | null {
		formError = null;
		const valid = validIngredientLines();
		const result = validateMeal(formName, valid, formInstructions, formPortions);
		if (!result.ok) {
			formError = t(result.messageKey);
			return null;
		}
		if (imageError) {
			formError = imageError;
			return null;
		}
		return {
			name: formName.trim(),
			ingredients: valid,
			instructions: formInstructions.trim(),
			portions: formPortions,
			image: formImage,
			removeImage,
		};
	}

	async function onSubmit() {
		const payload = buildPayload();
		if (!payload) return;
		try {
			await onsubmit(payload);
		} catch (err) {
			if (err instanceof ApiError && err.status === 409) {
				formError = t('errorDuplicateMeal');
			} else {
				formError = err instanceof ApiError && err.code === 'REQUEST_FAILED'
					? t('errorSaveFailed')
					: err instanceof Error ? err.message : String(err ?? '');
			}
		}
	}

	async function onCookClick() {
		const payload = buildPayload();
		if (!payload || !oncook) return;
		try {
			await oncook(payload);
		} catch (err) {
			formError = err instanceof Error ? err.message : String(err ?? '');
		}
	}
```

c) Actions row (lines 221-237): insert the cook button before the submit button and use the label override:

```svelte
		<div class="form-card__actions">
			{#if oncook}
				<button type="button" class="btn btn--ghost" onclick={onCookClick} disabled={submitting}>
					<Icon name="utensils" size={16} />
					{t('cookNowButton')}
				</button>
			{/if}
			<button type="submit" class="btn btn--primary" disabled={submitting || isDuplicate}>
				{#if editMode}
					<Icon name="check" size={16} />
					{t('buttonSave')}
				{:else}
					<Icon name="plus" size={16} />
					{submitLabel ?? t('buttonAdd')}
				{/if}
			</button>
```

Keep the edit-mode cancel button exactly as it is.

- [ ] **Step 6: Pass the overrides from the spontaneous page**

`web/src/routes/spontaneous/+page.svelte`, the `<MealForm>` usage (around line 165): add `submitLabel={t('buttonSave')}`. Do NOT pass `oncook` yet (Task 3).

- [ ] **Step 7: Run the affected specs to verify they pass**

Run: `cd tests && npx playwright test e2e/generate-meal.spec.ts`
Expected: PASS (7 tests; the save clicks now match "Save").

- [ ] **Step 8: Commit**

```bash
git add web/src/lib/MealForm.svelte web/src/lib/types.ts web/src/lib/i18n/en.ts web/src/lib/i18n/de.ts web/src/lib/i18n/types.ts web/src/routes/spontaneous/+page.svelte tests/e2e/generate-meal.spec.ts
git commit -m "feat: label spontaneous draft submit button Save"
```

---

### Task 2: Extract CookingView component (pure refactor)

**Files:**
- Create: `web/src/lib/components/CookingView.svelte`
- Modify: `web/src/routes/meals/[id]/+page.svelte` (replace lines 153-271 article block; delete `scaleQuantity` lines 34-42 and `desiredPortions` line 32; keep all other logic)

**Interfaces:**
- Produces (consumed by Task 3):
  ```ts
  // CookingView props
  {
    meal: Meal;
    imageUrl: string | null;               // data URL or API URL; null renders the placeholder
    plannedAt?: string | null;             // undefined hides the "planned" meta segment; null shows t('lastPlannedNever'); string shows the formatted date
    polishError?: string | null;           // renders the role=alert paragraph above the header when truthy
    heroActions?: Snippet;                 // rendered inside .cooking-view__hero-overlay; overlay omitted when absent
  }
  ```
- Internal (component-local): `desiredPortions` stepper state and `scaleQuantity` (moved verbatim from the detail page). Parents wrap usage in `{#key meal.id}` so the stepper resets when the meal changes.

- [ ] **Step 1: Create the component**

`web/src/lib/components/CookingView.svelte` — script:

```svelte
<script lang="ts">
	import Icon from '$lib/Icon.svelte';
	import { t, formatDate } from '$lib/i18n';
	import type { Meal, Snippet } from 'svelte';
	import { fly } from 'svelte/transition';
	import { tierDuration } from '$lib/motion';

	let {
		meal,
		imageUrl = null,
		plannedAt = undefined,
		polishError = null,
		heroActions,
	}: {
		meal: Meal;
		imageUrl?: string | null;
		plannedAt?: string | null;
		polishError?: string | null;
		heroActions?: Snippet;
	} = $props();

	let desiredPortions = $state<number | null>(meal.portions);

	function scaleQuantity(quantity: string | null, base: number, desired: number): string | null {
		if (!quantity || desired <= 0 || desired === base) return null;
		const match = quantity.match(/^(\d+\.?\d*)/);
		if (!match) return null;
		const num = parseFloat(match[1]);
		const scaled = num * (desired / base);
		const formatted = scaled % 1 === 0 ? scaled.toFixed(0) : scaled.toFixed(1);
		return quantity.replace(/^\d+\.?\d*/, formatted);
	}
</script>
```

Markup — copy the article block from `web/src/routes/meals/[id]/+page.svelte` lines 153-271 VERBATIM (same classes, same nesting), with these three adjustments:

1. Hero image: `src={imageUrl}` guarded by `{#if imageUrl}` instead of `{#if meal.has_image}` / `mealImageUrl(meal.id)`.
2. Hero overlay: wrap the overlay `<div class="cooking-view__hero-overlay">…</div>` in `{#if heroActions}` and replace its three hard-coded buttons with `{@render heroActions()}`.
3. Meta line: render the `last_planned_at` segment only when `plannedAt !== undefined`:

```svelte
			<p class="cooking-view__meta">
				<span>{meal.ingredients.length === 1 ? t('ingredientCountOne') : t('ingredientCount', { count: String(meal.ingredients.length) })}</span>
				{#if plannedAt !== undefined}
					<span class="cooking-view__meta-sep" aria-hidden="true">·</span>
					<span>{plannedAt ? t('lastPlanned', { date: formatDate(plannedAt, { month: 'short', day: 'numeric', year: 'numeric' }) }) : t('lastPlannedNever')}</span>
				{/if}
			</p>
```

Keep `{#if polishError}<p class="cooking-view__polish-error" role="alert">{polishError}</p>{/if}` in the same position (between `</figure>` and the header). Keep the `in:fly` transition on the article. The component needs no `<style>` block — all styles live in `web/src/app.css` under the `.cooking-view__*` classes.

- [ ] **Step 2: Rewire the meal-detail page**

`web/src/routes/meals/[id]/+page.svelte`:

a) Imports: remove `fly` if now unused, remove `Icon` if unused elsewhere in the page (it IS still used in modals — keep), add `import CookingView from '$lib/components/CookingView.svelte';`. Delete `desiredPortions` (line 32) and `scaleQuantity` (lines 34-42). Delete `desiredPortions = meal?.portions ?? null;` from `loadMeal` (line 55).

b) Replace the `{:else if meal}` block (lines 152-272) with:

```svelte
	{:else if meal}
		{#key meal.id}
			<CookingView
				{meal}
				imageUrl={meal.has_image ? mealImageUrl(meal.id) : null}
				plannedAt={meal.last_planned_at}
				{polishError}
			>
				{#snippet heroActions()}
					<button
						type="button"
						class="btn btn--ghost cooking-view__action-btn"
						aria-label={t('buttonEdit')}
						title={t('buttonEdit')}
						onclick={editMeal}
					>
						<Icon name="pen-line" size={16} />
					</button>
					{#if hasLlmConfig}
						<button
							type="button"
							class="btn btn--ghost cooking-view__action-btn"
							aria-label={t('buttonPolish')}
							title={t('buttonPolish')}
							onclick={doPolish}
						>
							<Icon name="sparkles" size={16} />
						</button>
					{/if}
					<button
						type="button"
						class="btn btn--danger-ghost cooking-view__action-btn"
						aria-label={t('buttonDelete')}
						title={t('buttonDelete')}
						onclick={openDelete}
					>
						<Icon name="trash-2" size={16} />
					</button>
				{/snippet}
			</CookingView>
		{/key}
	{/if}
```

(Copy the exact three buttons from the current overlay, lines 169-204, including `disabled={polishing}` if present on any of them — read the current file before editing.)

- [ ] **Step 3: Verify no behavior change**

Run: `cd web && npm run check` → Expected: 0 errors, 10 pre-existing warnings.
Run: `cd tests && npx playwright test e2e/cooking-view.spec.ts e2e/portions.spec.ts e2e/edit-meal-detail.spec.ts e2e/edit-meal-full.spec.ts`
Expected: all PASS — these exercise `.cooking-view__*` markup, stepper behavior, scaling, and edit/polish/delete flows through the extracted component.

- [ ] **Step 4: Commit**

```bash
git add web/src/lib/components/CookingView.svelte web/src/routes/meals/[id]/+page.svelte
git commit -m "refactor: extract CookingView from meal detail page"
```

---

### Task 3: Cook-now flow (sessionStorage draft + /spontaneous/cook route)

**Files:**
- Modify: `web/src/routes/spontaneous/+page.svelte` (add `oncook`, helper, sessionStorage write)
- Create: `web/src/routes/spontaneous/cook/+page.svelte`
- Modify: `tests/e2e/generate-meal.spec.ts` (new test)
- Modify: `.spec/spontaneous-on-the-fly-meal-generation.md` (FRs, success criteria)

**Interfaces:**
- Consumes: `CookingView` props from Task 2, `MealForm` `oncook`/`submitLabel` from Task 1, `MealFormPayload` type.
- Produces: `sessionStorage` key `yummybox-cook-draft` containing JSON `{ name: string; ingredients: NewIngredientLine[]; instructions: string; portions: number | null; imageDataUrl: string | null }`.

- [ ] **Step 1: Write the failing e2e test**

Append to `tests/e2e/generate-meal.spec.ts` (inside the describe, after the restore test):

```ts
	test('cooks the edited draft without persisting it', async ({ page }) => {
		await page.goto('/spontaneous');
		await configureMockProvider(page);
		await page.getByLabel(/ingredients/i).fill('flour\neggs');
		await page.getByRole('button', { name: /^Generate recipe$/ }).click();
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Mock Pasta');
		// Edits made in the form must carry over into cooking.
		await page.getByLabel('Name', { exact: true }).fill('Cooked Draft');
		await page.getByRole('button', { name: 'Cook now' }).click();
		await expect(page).toHaveURL(/\/spontaneous\/cook$/);
		await expect(page.locator('.cooking-view__name')).toHaveText('Cooked Draft');
		await expect(page.locator('.cooking-view__ingredient-list')).toContainText('flour');
		// Nothing was persisted.
		let res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(0);
		// Leaving the flow forgets the draft: the spontaneous page is fresh.
		await page.goto('/spontaneous');
		await expect(page.locator('.generate-draft')).toHaveCount(0);
		res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(0);
	});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd tests && npx playwright test e2e/generate-meal.spec.ts -g "cooks the edited draft"`
Expected: FAIL — no "Cook now" button exists yet.

- [ ] **Step 3: Implement the spontaneous page handoff**

`web/src/routes/spontaneous/+page.svelte`:

a) Add the helper above the component (script section, after `onSave`):

> **Implementation note:** The helper shipped as `downscaleImage` (canvas re-encode to a bounded JPEG, max edge 1280, quality 0.8) instead of the originally planned `fileToDataUrl` FileReader read: raw data URLs of generated JPEGs and user photos routinely exceed the ~5 MB sessionStorage quota. The code below is what shipped.

```ts
	const COOK_DRAFT_MAX_EDGE = 1280;

	// Bound the draft image before storing it: generated JPEGs and user photos
	// routinely exceed the ~5 MB sessionStorage quota as base64 data URLs, so
	// re-encode to a downscaled JPEG that always fits.
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

	async function onCook(payload: import('$lib/types').MealFormPayload) {
		cookError = null;
		try {
			const imageDataUrl = payload.image ? await downscaleImage(payload.image) : null;
			sessionStorage.setItem('yummybox-cook-draft', JSON.stringify({
				name: payload.name,
				ingredients: payload.ingredients,
				instructions: payload.instructions,
				portions: payload.portions,
				imageDataUrl,
			}));
		} catch {
			cookError = t('cookDraftSaveError');
			return;
		}
		await goto('/spontaneous/cook');
	}
```

b) On the `<MealForm>` usage: add `oncook={onCook}` next to `submitLabel={t('buttonSave')}`.

- [ ] **Step 4: Create the cook route**

`web/src/routes/spontaneous/cook/+page.svelte`:

```svelte
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
```

(`plannedAt` is omitted → the meta line shows only the ingredient count.)

- [ ] **Step 5: Run the new test to verify it passes**

Run: `cd tests && npx playwright test e2e/generate-meal.spec.ts -g "cooks the edited draft"`
Expected: PASS.

- [ ] **Step 6: Update the feature spec**

`.spec/spontaneous-on-the-fly-meal-generation.md`, under Functional Requirements add:

```markdown
- **FR-010**: In the spontaneous flow, the draft form's submit button is labeled "Save" (de: "Speichern"); the add-meal and edit-meal dialogs keep their existing labels.
- **FR-011**: The draft form offers a "Cook now" action (de: "Jetzt kochen") that opens the cooking view with the current, validated form values (including edits) without any database write. The draft is held in session-scoped storage and forgotten when the tab closes; the cooking view offers no edit/polish/delete actions.
```

Under Success Criteria add:

```markdown
- **SC-007**: Cooking a generated draft renders its name, ingredients, instructions, and portions in the cooking view while the meals list stays empty; the same draft can still be saved afterwards by returning to the form.
```

- [ ] **Step 7: Commit**

```bash
git add web/src/routes/spontaneous/+page.svelte web/src/routes/spontaneous/cook/+page.svelte tests/e2e/generate-meal.spec.ts .spec/spontaneous-on-the-fly-meal-generation.md
git commit -m "feat: cook spontaneous draft without persisting"
```

---

### Task 4: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Frontend gates**

Run: `cd web && npm run check` → Expected: 0 errors, 10 pre-existing warnings.
Run: `cd web && npm test` → Expected: 147/147 (i18n parity covers the 2 new keys).

- [ ] **Step 2: Fresh release build**

Run: `rm -rf web/build && cargo build --release` (from the repo root; required so the workflow e2e webServer serves the new embed — never skip deleting `web/build`).

- [ ] **Step 3: Full workflow e2e suite**

Run: `cd tests && npm test` → Expected: all pass (91 existing + 1 new cook test, 92 total). The mock LLM webServer on 127.0.0.1:18999 must be free (stop any hub-managed `mock-llm` process first if present).

- [ ] **Step 4: Visual e2e suite**

Run: `cd web && npm run test:e2e` → Expected: 6/6 (port 11341 must be free; stop the hub `yummybox` process if it is running).

- [ ] **Step 5: Manual browser check**

Restart the demo app (`hub restart yummybox`) and drive the flow once: `/spontaneous` → generate → edit name → "Cook now" → cooking view shows the edited draft → nav back → no meal in `/api/meals` → "Save" persists. Verify both light and dark theme and a <768px viewport for the two new controls.

- [ ] **Step 6: Commit any fixes from verification**

```bash
git add -A
git commit -m "chore: verification fixes for spontaneous cook flow"
```

---

## Self-Review Notes

- Spec coverage: FR-010 (Task 1), FR-011 + SC-007 (Task 3, steps 6 and e2e test), cook-route readability/forget semantics (Task 3 step 4: sessionStorage + fresh spontaneous page).
- Placeholder scan: none; all code blocks are concrete. The one instruction that says "read the current file before editing" (Task 2 step 2c) concerns copying existing buttons verbatim, not a missing detail.
- Type consistency: `MealFormPayload` defined in Task 1 and consumed by Task 3's `onCook`; `CookingView` props (`meal`, `imageUrl`, `plannedAt`, `polishError`, `heroActions`) defined in Task 2 and consumed by Task 3's cook page exactly.
- Regression risk check: Task 2 is a pure refactor gated by the four cooking e2e specs; Task 1 changes only the spontaneous draft label (modal specs unaffected); Task 3 adds a route and one test.
