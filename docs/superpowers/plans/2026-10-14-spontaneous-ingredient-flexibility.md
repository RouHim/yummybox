# Spontaneous Cooking: Ingredient Subset, Pantry Staples & Extra Instructions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Relax the spontaneous generation contract so the model may use a subset of the entered ingredients and may add a fixed pantry set, and add an optional extra-instructions field that the model treats as a binding constraint.

**Architecture:** The generation instruction (`GENERATE_SYSTEM_PROMPT` in `src/llm_import.rs`) is rewritten in place; a small pure helper composes the user text from ingredients plus extra instructions, and `generate_meal_via_llm` gains one parameter. The `/import/generate` multipart handler in `src/import.rs` parses and validates a new `extra_instructions` field and passes it through. The frontend adds the field to `generateMeal()` and to the `/spontaneous` page (textarea plus a pantry/subset hint), with strings in the existing en/de dictionaries. One new Playwright workflow test covers the UI happy path against the existing mock LLM.

**Tech Stack:** Rust (axum multipart, genai chat tool calls), Svelte 5 runes, TypeScript strict, Vitest, Playwright (workflow suite in `tests/`).

**Spec:** `.spec/spontanes-kochen-zutaten.md`

## Global Constraints

- Fixed pantry set, complete and exact: salt, pepper, oil, sugar, water, vinegar, flour, broth, herbs, spices. No other additions; no settings surface.
- Extra instructions: at most 500 characters counted in characters; empty or whitespace-only counts as absent and must not be sent.
- At least one ingredient (text or photo) remains required; extra instructions alone never enable the generate button.
- Existing limits and error handling unchanged: ingredients ≤ 20000 chars, ≤ 5 photos, ≤ 20 MB per image, 60 s LLM timeout, client request timeout 90 s, error codes `llm_timeout`, `llm_parse_failed`, `llm_api_key_missing`, `llm_not_configured`.
- No new crates or npm packages; no DB schema or migration changes; new validation failures use `AppError::BadRequest`.
- Backend strings English-only; UI strings in `web/src/lib/i18n/en.ts` + `de.ts` + the `types.ts` key union (parity test enforces both locales); no em dashes in UI strings.
- BDD test names: `given_<condition>_when_<action>_then_<outcome>`.
- Gates before done: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cd web && npm run check && npm test`, `cd tests && npm test`.

## Review Focus

1. A dietary constraint ("vegan", "no nuts") silently ignored → the user cooks something they cannot eat. Guard: the binding-constraint and conflict-precedence clauses (Task 1 test) plus the manual live smoke (Task 4).
2. Extra instructions dropped in wiring (page → `api.ts` → multipart → provider payload): the field looks functional but changes nothing. Pinned by Task 2's provider-payload test and Task 4's request-body assertion.
3. Photo-only generation (empty ingredients text) loses the extra instructions. Pinned by Task 2's `build_generate_hint` unit tests.
4. Duplicate ingredient lines when an entered ingredient is also a pantry member ("400 g Mehl" plus pantry flour). Guard: the single-listing rule already in the prompt; exercised in the manual live smoke (Task 4).
5. Oversized or whitespace-only extra instructions (a paste bypasses `maxlength`): server must reject > 500 before any LLM call, whitespace-only must behave as empty. Pinned by Task 2 (501 chars → 400) and Task 3 (whitespace omitted).

---

### Task 1: Rewrite the generation contract

**Files:**
- Modify: `src/llm_import.rs:347` (`GENERATE_SYSTEM_PROMPT`), inline `#[cfg(test)] mod tests` block (same file, starts near line 860).

**Interfaces:**
- Consumes: nothing.
- Produces: the new `GENERATE_SYSTEM_PROMPT` constant, consumed by `generate_meal_via_llm`; no signature change.

- [ ] **Step 1: Write the failing prompt-contract test**

Add to the `#[cfg(test)] mod tests` block in `src/llm_import.rs`:

```rust
#[test]
fn given_generate_prompt_when_inspected_then_contains_subset_pantry_and_constraint_clauses() {
    assert!(GENERATE_SYSTEM_PROMPT.contains("you do not have to use all of them"));
    assert!(GENERATE_SYSTEM_PROMPT
        .contains("salt, pepper, oil, sugar, water, vinegar, flour, broth, herbs, and spices"));
    assert!(GENERATE_SYSTEM_PROMPT.contains("binding constraints"));
    assert!(GENERATE_SYSTEM_PROMPT.contains("the requirement wins"));
    assert!(GENERATE_SYSTEM_PROMPT.contains("list every ingredient you use"));
    assert!(!GENERATE_SYSTEM_PROMPT.contains("must primarily use the provided ingredients"));
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test given_generate_prompt_when_inspected`
Expected: FAIL — the current prompt contains "must primarily use the provided ingredients".

- [ ] **Step 3: Replace `GENERATE_SYSTEM_PROMPT` with this exact text**

```rust
const GENERATE_SYSTEM_PROMPT: &str = "You are a creative cooking assistant. Create a recipe from the user's available ingredients (a text list, photos, or both). The recipe is built from the provided ingredients, but you do not have to use all of them: choose a sensible subset and leave out ingredients that do not fit the dish. Use at least one provided ingredient whenever any were given, and never use ingredients other than the provided ones plus these common pantry staples that most kitchens have: salt, pepper, oil, sugar, water, vinegar, flour, broth, herbs, and spices; list every ingredient you use, including the pantry staples, in the ingredients array. Preserve the exact quantities the user specified; assign plausible quantities to ingredients that have none. If the user adds extra instructions or requirements (for example a time limit, a dietary rule such as vegan, or an appliance such as a Thermomix), treat them as binding constraints and shape the recipe accordingly; when a requirement conflicts with a provided ingredient or a pantry staple, the requirement wins and the conflicting item is left out. If an ingredient appears in both the text list and the photos, list it once, using the quantity from the text list. Respond in the same language as the user's input. Never invent an image URL; always leave the imageUrl field empty. Call the extract_recipe tool with the result. Always call the tool.";
```

- [ ] **Step 4: Run the test, then the full Rust suite**

Run: `cargo test given_generate_prompt_when_inspected` → PASS.
Run: `cargo test` → all green (the prompt is not asserted anywhere else).

- [ ] **Step 5: Commit**

```bash
git add src/llm_import.rs
git commit -m "feat(generate): allow ingredient subsets and pantry staples in the generation contract"
```

---

### Task 2: Extra-instructions field through the backend

**Files:**
- Modify: `src/llm_import.rs` (`generate_meal_via_llm` at :354-392, new helper above it, inline tests)
- Modify: `src/import.rs` (const block at :263-268, `generate_meal` at :272-365)
- Modify: `src/routes_tests.rs` (helper `build_generate_multipart` at :1996, new tests next to the existing generate tests at :1725-2240)

**Interfaces:**
- Consumes: `GENERATE_SYSTEM_PROMPT` (Task 1).
- Produces:
  - `fn build_generate_hint(ingredients: Option<&str>, extra_instructions: Option<&str>) -> Option<String>` (private in `src/llm_import.rs`). Returns `None` when both are empty/whitespace; otherwise the trimmed ingredients, the block `Additional requirements:\n{extra}` when extra is present, joined by a blank line when both are present.
  - `pub async fn generate_meal_via_llm(target: &LlmTarget<'_>, model: &str, ingredients: Option<&str>, extra_instructions: Option<&str>, images: Vec<LlmImage>) -> Result<recipe::ImportDraft, AppError>`.
  - `const MAX_EXTRA_INSTRUCTIONS_CHARS: usize = 500;` in `src/import.rs`; multipart field name `extra_instructions`; 400 message `extra instructions must be at most 500 characters`.
  - Test helper `fn build_generate_multipart_with_extra(ingredients: Option<&str>, extra_instructions: Option<&str>, images: &[&[u8]]) -> (Vec<u8>, String)`; `build_generate_multipart` keeps its signature and delegates with `None`.

- [ ] **Step 1: Write the failing unit tests for `build_generate_hint`**

```rust
#[test]
fn given_ingredients_and_extra_when_build_generate_hint_then_requirements_appended() {
    let hint = build_generate_hint(Some("flour\neggs"), Some("max 30 minutes"))
        .expect("both inputs present");
    assert_eq!(hint, "flour\neggs\n\nAdditional requirements:\nmax 30 minutes");
}

#[test]
fn given_only_extra_when_build_generate_hint_then_requirements_only() {
    let hint = build_generate_hint(None, Some("vegan")).expect("extra present");
    assert_eq!(hint, "Additional requirements:\nvegan");
}

#[test]
fn given_only_ingredients_when_build_generate_hint_then_ingredients_unchanged() {
    let hint = build_generate_hint(Some("flour"), None).expect("ingredients present");
    assert_eq!(hint, "flour");
}

#[test]
fn given_only_whitespace_when_build_generate_hint_then_none() {
    assert!(build_generate_hint(Some("  \n"), Some("\t")).is_none());
    assert!(build_generate_hint(None, None).is_none());
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test build_generate_hint`
Expected: FAIL to compile — `build_generate_hint` does not exist.

- [ ] **Step 3: Implement the helper and change the signature**

```rust
fn build_generate_hint(
    ingredients: Option<&str>,
    extra_instructions: Option<&str>,
) -> Option<String> {
    let ingredients = ingredients.map(str::trim).filter(|s| !s.is_empty());
    let extra = extra_instructions.map(str::trim).filter(|s| !s.is_empty());
    match (ingredients, extra) {
        (None, None) => None,
        (Some(i), None) => Some(i.to_string()),
        (None, Some(e)) => Some(format!("Additional requirements:\n{e}")),
        (Some(i), Some(e)) => Some(format!("{i}\n\nAdditional requirements:\n{e}")),
    }
}
```

In `generate_meal_via_llm`, add the parameter after `ingredients` and build the content through the helper:

```rust
let user_content = build_user_content(
    build_generate_hint(ingredients, extra_instructions).as_deref(),
    &images,
);
```

- [ ] **Step 4: Run the unit tests, then the routes tests (compile check)**

Run: `cargo test build_generate_hint` → PASS.
Run: `cargo test` → FAILS to compile — `src/import.rs` calls `generate_meal_via_llm` with the old arity. That failure is the next step's red state.

- [ ] **Step 5: Write the failing route tests**

Add `build_generate_multipart_with_extra` next to `build_generate_multipart` (`src/routes_tests.rs:1996`): same body builder, but when `extra_instructions` is `Some(text)` it writes one more form part named `extra_instructions`; `build_generate_multipart` becomes a delegation with `None`.

Then two tests next to the existing generate tests:

```rust
#[tokio::test]
async fn given_extra_instructions_when_generate_meal_then_sent_to_the_provider() {
    // Same stub-server pattern as given_stored_custom_provider_when_generate_meal_then_draft_returned
    // (:1776-1860): accept one connection, read the request per Content-Length into a String,
    // then parse it as JSON for the canned response.
    // Body: build_generate_multipart_with_extra(Some("flour\neggs"), Some("max 30 minutes, vegan"), &[])
    // Assertions after the 200 response:
    //   raw_body.contains("Additional requirements")
    //   raw_body.contains("max 30 minutes, vegan")
    //   raw_body.contains("you do not have to use all of them")   // the Task 1 contract is live
}

#[tokio::test]
async fn given_extra_instructions_over_500_chars_when_generate_meal_then_400_before_any_llm_call() {
    // No provider seeded: the length check must run before effective_ai, so the
    // response is the length error, not llm_not_configured.
    // let long = "x".repeat(501);
    // Body: build_generate_multipart_with_extra(Some("flour"), Some(&long), &[])
    // Assertions: status 400; json["error"] == "extra instructions must be at most 500 characters".
}
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo test given_extra_instructions`
Expected: FAIL to compile (unknown field handling) or 400-with-wrong-message / missing assertion.

- [ ] **Step 7: Implement the handler changes in `src/import.rs`**

Add beside `MAX_INGREDIENTS_CHARS`:

```rust
/// Maximum length of the extra instructions field.
const MAX_EXTRA_INSTRUCTIONS_CHARS: usize = 500;
```

In `generate_meal`: collect `Some("extra_instructions")` into a `Option<String>` the same way as `ingredients`, then after the ingredients normalization add:

```rust
let extra_instructions = extra_instructions
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());

if let Some(extra) = &extra_instructions {
    if extra.chars().count() > MAX_EXTRA_INSTRUCTIONS_CHARS {
        return Err(AppError::BadRequest(
            "extra instructions must be at most 500 characters".into(),
        ));
    }
}
```

Pass `extra_instructions.as_deref()` into `generate_meal_via_llm` between `ingredients.as_deref()` and `images`.

- [ ] **Step 8: Run the tests**

Run: `cargo test given_extra_instructions` → PASS.
Run: `cargo test` → PASS, including the untouched generate tests (limits, images, keyless provider).
Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings` → clean.

- [ ] **Step 9: Commit**

```bash
git add src/llm_import.rs src/import.rs src/routes_tests.rs
git commit -m "feat(generate): accept extra instructions on the generate endpoint"
```

---

### Task 3: Frontend field, hint, and API client

**Files:**
- Modify: `web/src/lib/api.ts:184-189` (`generateMeal`)
- Modify: `web/src/lib/api.test.ts` (`describe('generateMeal')`, :673-702)
- Modify: `web/src/routes/spontaneous/+page.svelte` (state near :35-40, `onGenerate` :76-120, create card markup :258-292)
- Modify: `web/src/lib/i18n/en.ts`, `web/src/lib/i18n/de.ts` (generate block :94-113), `web/src/lib/i18n/types.ts` (key union :100-119)

**Interfaces:**
- Consumes: multipart field `extra_instructions` (Task 2).
- Produces:
  - `export async function generateMeal(ingredients: string, images: File[], extraInstructions = ''): Promise<ImportDraft>`
  - page state `let extraInstructions = $state('');`
  - i18n keys `generateExtraLabel`, `generateExtraPlaceholder`, `generateAssumptionsHint`.

- [ ] **Step 1: Write the failing vitest cases**

In `web/src/lib/api.test.ts`, inside `describe('generateMeal')`:

```ts
it('sends extra instructions when provided', async () => {
    mockResponse(200, { name: '', ingredients: [], instructions: '', imageBase64: null, portions: null, sourceUrl: null });
    await generateMeal('flour', [], 'maximal 30 Minuten, vegan');
    const fd = mockFetch.mock.calls[0][1].body as FormData;
    expect(fd.get('extra_instructions')).toBe('maximal 30 Minuten, vegan');
});

it('omits whitespace-only extra instructions', async () => {
    mockResponse(200, { name: '', ingredients: [], instructions: '', imageBase64: null, portions: null, sourceUrl: null });
    await generateMeal('flour', [], '   ');
    const fd = mockFetch.mock.calls[0][1].body as FormData;
    expect(fd.get('extra_instructions')).toBeNull();
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd web && npm test -- generateMeal`
Expected: FAIL — `extra_instructions` is `null` in the first case.

- [ ] **Step 3: Implement the client change**

```ts
export async function generateMeal(ingredients: string, images: File[], extraInstructions = ''): Promise<ImportDraft> {
	const form = new FormData();
	if (ingredients.trim()) form.set('ingredients', ingredients);
	if (extraInstructions.trim()) form.set('extra_instructions', extraInstructions);
	for (const img of images) form.append('image', img);
	return request<ImportDraft>('/api/import/generate', { method: 'POST', body: form }, 90_000);
}
```

- [ ] **Step 4: Run the vitest suite**

Run: `cd web && npm test` → PASS.

- [ ] **Step 5: Add the i18n strings**

`web/src/lib/i18n/en.ts`, beside the other `generate*` keys (after `generateWhatHint`):

```ts
	generateAssumptionsHint: 'Not every ingredient has to end up in the recipe: the AI may pick a sensible subset. Standard pantry items such as salt, pepper, oil, and sugar are assumed to be on hand.',
	generateExtraLabel: 'Extra instructions (optional)',
	generateExtraPlaceholder: 'e.g. max 30 minutes, vegan, Thermomix',
```

`web/src/lib/i18n/de.ts` (same position):

```ts
	generateAssumptionsHint: 'Nicht jede Zutat muss im Rezept landen: die KI darf eine sinnvolle Auswahl treffen. Standardzutaten wie Salz, Pfeffer, Öl und Zucker werden als vorhanden angenommen.',
	generateExtraLabel: 'Zusatzanweisungen (optional)',
	generateExtraPlaceholder: 'z. B. maximal 30 Minuten, vegan, Thermomix',
```

Add the three keys to the union in `web/src/lib/i18n/types.ts` beside the other `generate*` entries.

- [ ] **Step 6: Wire the page**

In `web/src/routes/spontaneous/+page.svelte`: add `let extraInstructions = $state('');` beside `ingredients`, pass the value in `onGenerate` (`await generateMeal(ingredients, images, extraInstructions)`), and render after the ingredients field, before the photos block:

```svelte
<label class="field create-card__field">
    <span class="field__label">{t('generateExtraLabel')}</span>
    <textarea
        bind:value={extraInstructions}
        placeholder={t('generateExtraPlaceholder')}
        rows="2"
        maxlength="500"
        disabled={generating}
        class="create-card__textarea"
    ></textarea>
</label>
<p class="create-card__hint">{t('generateAssumptionsHint')}</p>
```

- [ ] **Step 7: Run frontend checks**

Run: `cd web && npm run check` → 0 errors.
Run: `cd web && npm test` → PASS (dictionary parity test included).

- [ ] **Step 8: Commit**

```bash
git add web/src/lib/api.ts web/src/lib/api.test.ts web/src/lib/i18n web/src/routes/spontaneous/+page.svelte
git commit -m "feat(generate): extra instructions field and pantry hint on the spontaneous page"
```

---

### Task 4: E2E happy path and full gate

**Files:**
- Modify: `tests/e2e/generate-meal.spec.ts` (new test inside `test.describe('Generate meal page')`)

**Interfaces:**
- Consumes: Tasks 1-3 shipping behavior; helpers `configureMockProvider`, `resetMeals`, `resetSettings`, `setLocale` from `tests/e2e/_helpers.ts`; mock LLM at `tests/e2e/mock-llm.mjs` (returns "Mock Pasta").

- [ ] **Step 1: Write the new E2E test**

```ts
test('generates with extra instructions and shows the pantry hint', async ({ page }) => {
    await page.goto('/spontaneous');
    await expect(
        page.getByText(/Standard pantry items such as salt, pepper, oil, and sugar/)
    ).toBeVisible();
    await configureMockProvider(page);
    await page.getByLabel(/ingredients/i).fill('flour\neggs');
    await page.getByLabel(/extra instructions/i).fill('max 30 minutes, vegan');
    const sent = page.waitForRequest(
        (r) => r.url().includes('/api/import/generate') && r.method() === 'POST'
    );
    await page.getByRole('button', { name: /^Generate recipe$/ }).click();
    const request = await sent;
    expect(request.postData() ?? '').toContain('extra_instructions');
    expect(request.postData() ?? '').toContain('max 30 minutes, vegan');
    await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Mock Pasta');
    // Removing an ingredient line must survive into the saved meal (SC-006):
    // drop the second draft row (eggs) the mock returned.
    await page.getByRole('button', { name: 'Remove', exact: true }).nth(1).click();
    await expect(page.getByLabel(/Ingredient name/)).toHaveCount(1);
    await page.getByRole('button', { name: /^(Save|Speichern)$/ }).click();
    await expect(page).toHaveURL(/\/meals/);
    const saved = page.getByRole('listitem').filter({ hasText: 'Mock Pasta' });
    await expect(saved).toContainText('flour');
    await expect(saved).not.toContainText('eggs');
});
```

- [ ] **Step 2: Run it to verify it fails against the current build**

Run: `cd tests && npx playwright test e2e/generate-meal.spec.ts -g "extra instructions"`
Expected: FAIL while Tasks 1-3 are incomplete (label `Extra instructions` not found).

- [ ] **Step 3: Run it again after Tasks 1-3, then the whole workflow suite**

Run: `cd tests && npx playwright test e2e/generate-meal.spec.ts` → all 10 tests PASS.
Run: `cd tests && npx playwright test e2e/i18n.spec.ts e2e/import-llm.spec.ts` → PASS (string audit and import flow unaffected).
Run: `cd tests && npm test` → full workflow suite PASS.

- [ ] **Step 4: Manual live smoke (requires a real provider; not CI-deterministic)**

With a configured provider, generate twice: once with an over-supplied list (6+ ingredients) and once with 3 ingredients plus `maximal 30 Minuten, vegan`. Confirm: the first draft omits at least one ingredient without an error; the second honors the time limit and the dietary rule and lists any pantry additions as normal lines. Record the observed result in the PR description.

- [ ] **Step 5: Run every gate**

Run: `cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test`
Run: `cd web && npm run check && npm test`
Run: `cd tests && npm test`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add tests/e2e/generate-meal.spec.ts
git commit -m "test(e2e): cover extra instructions and pantry hint on the spontaneous page"
```
