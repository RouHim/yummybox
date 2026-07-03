# Feature Specification: Duplicate Meal Check on Import

**Created**: 2026-07-03
**Status**: Approved
**Input**: Duplicate check when importing meals — prevent identical meals from being added

## Goal

Prevent duplicate meals from being added to MealMe across all meal-persistence paths. A meal is considered a duplicate when its name matches an existing meal's name after case-insensitive, whitespace-normalized comparison. Direct-persist paths block duplicates: bulk URL import skips and reports them in the failure list; manual creation and meal update reject them with HTTP 409; the existing ZIP import path's silent-skip behavior is preserved but its name normalization becomes case-insensitive for consistency. Draft import review paths show an advisory inline warning and disable Save while the name collides. No database schema change or UNIQUE constraint is introduced — the check is application-level.

## User Scenarios

### Scenario 1 - Bulk URL import skips duplicates (P1)

A user pastes several recipe URLs for bulk import. One URL resolves to a recipe whose name matches an existing meal (case-insensitive). Another URL in the same batch resolves to the same recipe name as a URL earlier in the batch. The matching URLs are not persisted; they appear in the bulk result's `failed` list with reason `duplicate`. Non-duplicate URLs persist normally, the modal closes, and the overview list refreshes.

**Acceptance**

1. Given a meal named "Spaghetti Carbonara" already exists, when the user submits a bulk import containing a URL whose parsed recipe name normalizes to "spaghetti carbonara" (case-insensitive), then that URL is not persisted, it appears in `BulkImportResult.failed` with `reason: "duplicate"`, and the response HTTP status remains 200.
2. Given a bulk import batch of three URLs where the second and third URLs both resolve to the recipe name "Tomato Soup", when the import is processed sequentially, then the second URL is persisted normally and the third URL is added to `failed` with `reason: "duplicate"` (within-batch detection via re-querying the DB after each insert).
3. Given a bulk import where some URLs are duplicates and others are not, when the import completes, then all non-duplicate meals are persisted and visible in the refreshed overview list, and all duplicate URLs are listed in the modal's failures block with the duplicate reason.

### Scenario 2 - Manual meal creation rejects duplicates (P1)

A user manually creates a new meal via the add-meal form. The submitted name matches an existing meal's name (case-insensitive, whitespace-normalized). The request is rejected with HTTP 409 Conflict. No meal is persisted. The inline error message names the constraint so the user can rename and retry.

**Acceptance**

1. Given a meal named "Pancakes" already exists, when the user submits a new meal with name "pancakes" (or "  PANCAKES  "), then the backend returns HTTP 409 with body `{"error":"a meal with this name already exists"}` and no meal is inserted.
2. Given the create request returns 409, when the frontend handles the error, then an inline error message is shown in the add-meal form (reusing the existing validation-error display pattern), the modal stays open, and the user can rename and retry.
3. Given no existing meal has the submitted name, when the user creates a meal, then the meal persists normally with HTTP 201 (existing behavior unchanged).

### Scenario 3 - Meal update rejects rename to a duplicate name (P1)

A user edits an existing meal and changes its name to match another meal's name (case-insensitive). The update is rejected with HTTP 409. The original meal is unchanged. Renaming a meal to its own current name (case-only or whitespace-only change) is allowed because the collision check excludes the meal being updated (by id).

**Acceptance**

1. Given meals "Lasagna" (id 5) and "lasagna" (id 9) exist as separate meals, when the user updates meal 5's name to "lasagna", then the backend returns HTTP 409, and meal 5's name is unchanged.
2. Given meal "Tacos" (id 3), when the user updates meal 3's name to "tacos" (the same meal by id), then the update succeeds normally (HTTP 200), because the duplicate check excludes id 3.
3. Given meal "Tacos" (id 3) and meal "Burritos" (id 7), when the user updates meal 3's name to "Burritos", then the backend returns HTTP 409, and meal 3 retains its original name.

### Scenario 4 - Draft import review form shows advisory duplicate warning (P2)

A user imports a recipe via the URL, paste, or LLM import tab. The parsed draft's name matches an existing meal's name (case-insensitive). The review form shows an inline warning and disables the Save button until the user changes the name to something unique. The import request itself still returns the draft with HTTP 200 (no backend duplicate check on draft endpoints).

**Acceptance**

1. Given a meal named "Risotto" already exists, when the user imports a recipe (via URL, paste, or LLM) whose parsed name normalizes to "risotto" (case-insensitive), then the import request succeeds with HTTP 200 and returns the draft as usual, and the review form displays an inline duplicate warning message.
2. Given the review form is showing the duplicate warning and Save is disabled, when the user edits the name field to a non-colliding name, then the warning clears and Save becomes enabled.
3. Given the review form is showing the duplicate warning, when the user attempts to submit with the colliding name still in place, then Save remains disabled — and if submit is reached by other means, the backend returns 409 (Scenario 2 contract holds).

### Scenario 5 - ZIP import duplicate normalization alignment (P2)

The existing ZIP import path (`POST /api/import/zip`) already skips duplicate meal names with a `skipped` counter. Its name normalization becomes case-insensitive to match the new duplicate key. The silent-skip + `skipped` counter behavior is preserved (duplicates are NOT added to the `failed` list on the ZIP path). This scenario ensures consistency: a meal named "Pizza" and a ZIP entry named "pizza" are now treated as duplicates where previously they were not.

**Acceptance**

1. Given a meal named "Pizza" exists, when the user imports a ZIP containing a recipe named "pizza" (different case), then that recipe is skipped (increments `skipped`, not added to `failed`) — previously it would have been inserted because the old normalization did not lowercase.
2. Given a meal named "Pizza" exists, when the user imports a ZIP containing a recipe named "Pizza" (same casing), then the behavior is unchanged: the recipe is skipped (existing behavior).
3. Given the ZIP import completes with duplicates, when the result is shown, then the existing `importZipResultsSuccess` message displays the created count and skipped count as before.

## Functional Requirements

- **FR-001**: The system MUST define a case-insensitive, whitespace-normalized meal-name comparison key. The normalization is: trim, lowercase, then collapse internal whitespace runs to single spaces. Backend and frontend apply the same normalization. The existing `db::normalize_ingredient_name` (`src/db.rs:81`, currently `split_whitespace().join(" ")` with no case folding) MUST be updated to lowercase, OR a new `normalize_meal_name` function MUST be added that performs trim + lowercase + whitespace normalization. The choice is a /plan decision; the spec requires the observable: case-insensitive meal-name dedup. If `normalize_ingredient_name` is updated in place, ingredient dedup also becomes case-insensitive (acceptable, consistent). The existing ZIP import path (`src/export_import.rs:357`) uses this function and MUST use the case-insensitive version after the change.
- **FR-002**: The system MUST add an application-level duplicate-name lookup in the persistence layer (`src/db.rs`) — a query or function that checks whether a meal with a given normalized name exists, optionally excluding a specific meal id (for the update path). The existing `db::list_meals` + iterate pattern (used in `src/export_import.rs:357-362`) is acceptable; a targeted SQL query (`SELECT 1 FROM meals WHERE LOWER(name) = ?1 [AND id != ?2] LIMIT 1`) is also acceptable — the choice is a /plan decision.
- **FR-003**: `POST /api/meals` (handler `create_meal`, `src/routes.rs:90`) MUST check for a duplicate normalized name before calling `db::insert_meal`. If a duplicate exists, the handler MUST return a new `AppError` variant mapping to HTTP 409 Conflict with body `{"error":"a meal with this name already exists"}`. No meal is inserted.
- **FR-004**: `PUT /api/meals/:id` (handler `update_meal`, `src/routes.rs:166`) MUST check for a duplicate normalized name before calling `db::update_meal`, excluding the meal being updated (by id). If a different meal with the colliding name exists, the handler MUST return HTTP 409 with the same structured body. The meal is not modified.
- **FR-005**: The `AppError` enum (`src/error.rs:9`) MUST gain a new variant for the duplicate-name conflict. The variant MUST map to HTTP 409 Conflict in the `IntoResponse` impl (`src/error.rs:44`), producing a JSON body `{"error":"a meal with this name already exists","code":null}` following the existing pattern. The exact variant name is a /plan decision.
- **FR-006**: `POST /api/import/bulk` (`import_bulk` / `process_single_url`, `src/routes.rs:619`/`655`) MUST check the parsed draft's normalized name against existing meals AND meals persisted earlier in the same batch, after `recipe::fetch_and_parse` succeeds and before `db::insert_meal`. If a duplicate is found, the URL MUST be added to `failed` with `reason: "duplicate"` and the meal MUST NOT be persisted. The bulk request HTTP status remains 200. The existing `BulkImportFailure { url, reason }` shape (`src/model.rs:103`) is reused unchanged — only a new `reason` literal is added.
- **FR-007**: `POST /api/import/url`, `POST /api/import/paste`, and `POST /api/import/llm` (draft import endpoints) MUST NOT perform the duplicate check at the backend. They continue to return `ImportDraft` with HTTP 200. Duplicate detection on these paths is frontend-only (FR-009).
- **FR-008**: The existing ZIP import path (`POST /api/import/zip`, `src/export_import.rs:199`) MUST use the case-insensitive normalization (FR-001) for its duplicate check. Its existing silent-skip + `skipped` counter behavior is preserved — duplicates are NOT added to `ZipImportFailure`/`failed`; they increment `skipped`. No structural change to `ZipImportResult { created, skipped, failed }`.
- **FR-009**: The frontend create-meal and update-meal flows MUST surface the HTTP 409 duplicate error inline, reusing the existing error-display pattern (same mechanism used for validation errors), with a user-facing message from a new i18n key. The frontend bulk-result reason mapping (`web/src/routes/meals/+page.svelte:631`, currently a ternary over `fetch failed` / `no recipe found` / else `importBulkReasonValidation`) MUST be extended to handle `reason === "duplicate"` with a new `importBulkReasonDuplicate` i18n key.
- **FR-010**: The draft import review form (populated by URL/paste/LLM import) MUST, after the draft is loaded, detect whether the parsed name collides with an existing meal (case-insensitive) and, if so, display an inline warning message and disable the Save button. The warning clears and Save re-enables once the name no longer collides. The mechanism (a lightweight check endpoint, reusing the already-loaded meals list, or catching 409 on submit) is a /plan decision; the spec requires the observable UX: advisory warning + disabled Save while colliding.
- **FR-011**: New i18n keys MUST be added to both `en.ts` and `de.ts` (`web/src/lib/i18n/`): (a) `importBulkReasonDuplicate` for the bulk-failure reason label, (b) a duplicate-warning string for the draft review form, (c) a duplicate-error string for create/save 409 responses. German translations mirror the English meaning. The keys are added to the `TranslationKey` union type in `web/src/lib/i18n/types.ts`.
- **FR-012**: No UNIQUE constraint, index, or migration is added to the `meals` table. The duplicate check is application-level only, run before any write. Existing duplicate meals in the database are left untouched (no backfill migration).
- **FR-013**: The duplicate check MUST NOT alter the ingredients, instructions, or image of any meal. It is purely a name-existence check that gates the existing insert/update logic.

## Key Entities

- **Meal** (existing, `src/model.rs:5`): `id`, `name`, `ingredients`, `instructions`, `last_planned_at`, `created_at`, `updated_at`, `has_image`. Unchanged in structure. The `name` field is the basis for the normalized duplicate key.
- **BulkImportFailure** (existing, `src/model.rs:103`): `{ url: String, reason: String }`. The `reason` field gains a new literal value `"duplicate"`; no structural change.
- **ZipImportResult** (existing, `src/model.rs`): `{ created: Vec<Meal>, skipped: usize, failed: Vec<ZipImportFailure> }`. Unchanged — the ZIP path's `skipped` counter already handles duplicates silently.
- **AppError** (existing, `src/error.rs:9`): gains a new variant for the duplicate-name conflict mapping to HTTP 409. The exact variant name is a /plan decision; its behavior (status code + JSON body) is fixed by FR-005.

## Edge Cases

- **Case-only or whitespace-only name differences**: "Pancakes", "pancakes", "  PANCAKES  ", "  Pan   Cakes  " all normalize to the same key (trim + lowercase + whitespace-collapse) and are treated as duplicates. This is a change from the existing `normalize_ingredient_name` behavior (which did not lowercase). The ZIP import path now also treats case-variant names as duplicates where it previously did not.
- **Unicode case folding**: Rust `str::to_lowercase()` and JavaScript `String.prototype.toLowerCase()` both use Unicode case folding; sufficiently consistent for a single-user local application. No locale-specific tailoring.
- **Within-batch duplicates in bulk import**: meals are inserted sequentially and each `insert_meal` commits its own transaction (`src/db.rs:312`, confirmed: begins, writes, commits, returns `find_meal`). A later URL in the same batch re-queries the DB (or re-loads the meal list) and sees meals from earlier URLs in the same batch. The existing `export_import.rs` ZIP path already relies on this same re-query pattern (`list_meals` after each skip/insert).
- **Bulk URL whose recipe fails to parse**: if `recipe::fetch_and_parse` returns an error, the URL goes to `failed` with the existing reason (`fetch failed` / `no recipe found`) — the duplicate check is never reached because the name is unknown. Existing behavior unchanged.
- **Update to the same meal's own name**: renaming meal id 3 from "Tacos" to "tacos" (case-only change) MUST succeed because the duplicate check excludes id 3. Only collisions with a *different* meal are rejected.
- **Existing duplicate meals already in the database**: no migration removes or surfaces them. Editing one of two colliding meals triggers the duplicate check only if the new name collides with a *third* meal or the other duplicate (by id). Saving an unchanged colliding meal (editing id 3 "Tacos" while id 9 "tacos" exists, name unchanged) MUST succeed because the check excludes id 3.
- **Draft import with a colliding name**: the import request returns 200 (no backend check on draft endpoints). The warning is frontend-only. If the user navigates away and back, the warning re-evaluates based on the current name field.
- **Bulk import where ALL URLs are duplicates**: all URLs land in `failed` with `reason: "duplicate"`, `created` is empty, the modal stays open showing the failures (consistent with the existing zero-created behavior from `unify-url-bulk-import.md` FR-004).
- **ZIP import with a case-variant duplicate**: previously, "Pizza" (existing) and "pizza" (ZIP entry) were NOT duplicates because `normalize_ingredient_name` did not lowercase. After FR-001/FR-008, they ARE duplicates. The ZIP entry is skipped (increments `skipped`). This is a behavior change for the ZIP path, accepted per the user's decision for cross-path consistency.
- **Concurrent requests with the same name in two separate bulk/ZIP imports**: without a UNIQUE constraint, a race could result in two meals with the same name. Accepted limitation of the application-level check for a single-user local application.
- **Empty or whitespace-only name**: the existing `validate_meal` (`src/db.rs:91`) rejects empty/whitespace-only names before the duplicate check runs; the duplicate check only sees non-empty trimmed names.

## Assumptions

- The normalized-name duplicate key is trim + lowercase + whitespace-collapse, applied identically on the backend (Rust: `trim()` + `to_lowercase()` + `split_whitespace().join(" ")`) and frontend (TypeScript: `trim()` + `toLowerCase()` + whitespace-collapse). No fuzzy, phonetic, or approximate matching.
- Whether to update `db::normalize_ingredient_name` in place (making ingredient dedup also case-insensitive — consistent, low-risk) or add a separate `normalize_meal_name` function is a /plan decision. The spec requires only that meal-name dedup is case-insensitive and that the existing ZIP import path uses the same case-insensitive normalization. Updating in place is the simpler path and affects ingredient dedup consistently; this is the recommended default.
- The duplicate check is a single additional `SELECT` or a `list_meals` re-query run before `INSERT`/`UPDATE`; it does not change the transaction structure of `insert_meal` or `update_meal` beyond the pre-check. The existing ZIP path already uses `list_meals` + iterate (`src/export_import.rs:357-362`) and that pattern is acceptable for the bulk path too.
- No UNIQUE database constraint is added because the draft-path advisory UX and the application-level 409 together cover the user-visible behavior, and a UNIQUE constraint would make existing duplicate data inconsistent with the schema.
- The new `AppError` variant's message is the literal `a meal with this name already exists`. The bulk-failure reason literal is `"duplicate"`. The i18n key for the bulk reason label is `importBulkReasonDuplicate`.
- The within-batch duplicate detection relies on each `insert_meal` committing its own transaction before the next URL is processed (confirmed by reading `src/db.rs:312`). No change to this commit behavior is needed.
- The draft-review-form duplicate-warning UX (FR-010) does not require a new dedicated backend endpoint if an existing mechanism suffices (e.g., querying the meals list the frontend already loads on page mount, or catching the 409 on submit). The choice is deferred to `/plan`; the spec only fixes the observable behavior.
- Major recipe managers (Mealie, Tandoor) allow duplicate recipe names and deduplicate imports by source URL. This spec uses name-based deduplication because MealMe does not track a source URL per meal; this is a deliberate product choice per the user's decision.
- The existing ZIP import's `skipped` counter (silent skip, not in `failed`) is preserved as-is. The user's "report in failed" choice applies to the bulk URL import path only. Whether the ZIP path should ALSO move duplicates into `failed` is out of scope for this spec.
- Existing Rust tests, Vitest tests, and Playwright E2E tests continue to pass after the change, with new tests added for the duplicate-check behavior (409 on create, 409 on update-to-colliding-name, bulk `reason: "duplicate"`, draft-form warning, ZIP case-variant skip). The bulk-failure reason mapping in the E2E bulk import spec is extended for `duplicate`. The existing ZIP duplicate test (`import_skips_duplicate_names`, `src/export_import.rs:927`) is updated to use case-variant names if it does not already.

## Success Criteria

- **SC-001**: Submitting a bulk import where one URL resolves to a recipe whose normalized name matches an existing meal (case-insensitive) results in that URL appearing in `BulkImportResult.failed` with `reason: "duplicate"`, no meal is inserted for it, the HTTP response is 200, and non-duplicate URLs in the batch are persisted.
- **SC-002**: Submitting a bulk import where two URLs in the same batch resolve to the same recipe name (case-insensitive) results in the first URL being persisted and the second URL appearing in `failed` with `reason: "duplicate"` (within-batch detection).
- **SC-003**: Submitting `POST /api/meals` with a name that matches an existing meal (case-insensitive, whitespace-normalized) returns HTTP 409 with body `{"error":"a meal with this name already exists","code":null}` and no meal is inserted.
- **SC-004**: Submitting `PUT /api/meals/:id` that renames a meal to a name colliding with a different meal returns HTTP 409; renaming a meal to its own current name (case-only/whitespace-only change) returns HTTP 200.
- **SC-005**: Importing a recipe via URL, paste, or LLM whose name matches an existing meal (case-insensitive) returns the draft with HTTP 200 and the frontend review form displays an inline duplicate warning with the Save button disabled.
- **SC-006**: When the user edits the name in the draft review form to a non-colliding value, the duplicate warning clears and the Save button becomes enabled.
- **SC-007**: Importing a ZIP containing a recipe whose name is a case-variant of an existing meal (e.g., existing "Pizza", ZIP entry "pizza") results in that recipe being skipped (incrementing `skipped`, not in `failed`) — the case-insensitive normalization now catches this where the old normalization did not.
- **SC-008**: New i18n keys exist in both `en.ts` and `de.ts` for the bulk `duplicate` reason label, the draft-form duplicate warning, and the create/save 409 error message. German translations mirror the English meaning.
- **SC-009**: No UNIQUE constraint, index, or migration is added to the `meals` table; existing duplicate meals in the database are untouched.
- **SC-010**: All existing Rust tests (`cargo test`), Vitest tests (`cd web && npm test`), and Playwright E2E suites pass after the change, with new tests added covering the duplicate-check behavior on each path.
