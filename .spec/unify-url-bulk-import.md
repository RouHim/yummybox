# Feature Specification: Unify URL and Bulk URL Import into Direct-Persist Input

**Created**: 2026-07-03
**Status**: Approved
**Input**: Unify URL import / bulk URL import — one view per meal, 1+n show meal overview, no longer draft mode after add

## Goal
Replace the separate "From link" (draft→review) and "Bulk URL" (direct-persist) import tabs in the add-meal modal with a single "Import URLs" tab. Recipe URLs (1–50, one per line) persist directly via the existing `POST /api/import/bulk` endpoint — no draft/review step. When exactly one meal is created with zero failures, navigate to that meal's detail view (`/meals/:id`). When multiple meals are created, close the modal and refresh the overview list. The LLM import tab and the manual meal-entry form remain unchanged.

## User Scenarios

### Scenario 1 - Import a single recipe URL (P1)
A user finds one recipe URL and wants it in MealMe. They open the add-meal modal, paste the single URL into the unified "Import URLs" textarea, and click Import. The meal is fetched, parsed, and persisted directly (no draft form to review). The user lands on the new meal's detail view (`/meals/:id`).

**Acceptance**
1. Given the add-meal modal is open, when the user pastes exactly one valid recipe URL and clicks Import, then `POST /api/import/bulk` is called with a single-element `urls` array, the meal is persisted, and the user is navigated to `/meals/<id>` (the meal detail view) for the newly created meal.
2. Given the unified textarea contains one URL, when the import completes with exactly one created meal and zero failures, then the add-meal modal closes and the meal detail view is displayed.
3. Given the import request fails (network error or non-2xx response), when the error is returned, then an inline error message is shown in the modal and the modal stays open to retry.

### Scenario 2 - Import multiple recipe URLs (P1)
A user has several recipe URLs and wants to add them all at once. They paste multiple URLs (one per line, up to 50) into the unified textarea and click Import. All valid URLs persist directly. The modal closes and the overview list refreshes showing the newly created meals.

**Acceptance**
1. Given the add-meal modal is open, when the user pastes two or more valid recipe URLs (one per line) and clicks Import, then `POST /api/import/bulk` is called with the full `urls` array, all valid meals are persisted, the modal closes, and the overview list is refreshed with the new meals visible.
2. Given the textarea contains more than 50 non-empty URLs, when the user attempts to import, then an error message is shown (no API call made) and the modal stays open.

### Scenario 3 - Mixed success and failures (P2)
A user pastes several URLs; some succeed, some fail (site blocks the fetch, no schema.org Recipe found, validation error). The successfully created meals are persisted and appear in the refreshed overview list. The modal stays open showing a results summary: the count of created meals plus a list of failed URLs with their failure reasons.

**Acceptance**
1. Given the textarea contains URLs where some succeed and some fail, when the import completes, then the overview list is refreshed with the created meals, and the modal shows a results block listing the success count and each failed URL with its reason (`fetch failed`, `no recipe found`, or `validation failed`).
2. Given the import produces exactly one created meal and one or more failures, when the result is processed, then the modal closes and the user is navigated to `/meals/<id>` for the single created meal.
3. Given the import produces zero created meals (all URLs failed), when the result is processed, then the failures are listed inline in the modal, the overview list refresh is not required (no new meals), and the modal stays open for the user to retry or start a new batch.

### Scenario 4 - LLM import tab unchanged (P2)
The "AI import" tab continues to parse via the LLM endpoint, return a draft, populate the manual entry form for review, and require an explicit save. This flow is not affected by the URL unification.

**Acceptance**
1. Given the add-meal modal is open, when the user switches to the "AI import" tab, then the existing LLM provider/model selector and hint textarea are shown, the import returns a draft that populates the manual form, and the meal is persisted only when the user explicitly saves.

### Scenario 5 - Manual meal entry unchanged (P2)
The manual meal-entry form (name, ingredients, instructions, image upload) remains available in the add-meal modal for users who type their own meal without importing. The form persists directly on submit.

**Acceptance**
1. Given the add-meal modal is open, when the user fills in name, ingredients, and instructions manually and submits, then the meal is created via `POST /api/meals`, the modal closes, and the overview list refreshes with the new meal visible.

## Functional Requirements
- **FR-001**: The add-meal modal MUST present exactly two import tabs: "Import URLs" (the unified tab) and "AI import". The separate "From link" and "Bulk URL" tabs are replaced by the single "Import URLs" tab.
- **FR-002**: The "Import URLs" tab MUST provide a single `<textarea>` where the user pastes recipe URLs, one per line. Empty lines and surrounding whitespace per line are ignored. A non-URL input (no `http://` or `https://` prefix) is treated as invalid and surfaced as a client-side validation error before any API call.
- **FR-003**: The "Import URLs" tab MUST call the existing `importBulk({ urls })` frontend API function (which posts to `POST /api/import/bulk`). No new backend endpoint is required.
- **FR-004**: After a successful bulk import, the frontend MUST apply this navigation rule based on the `BulkImportResult`:
  - `created.length === 1 && failed.length === 0` → close the modal and `goto('/meals/<created[0].id>')` (meal detail view).
  - `created.length > 1` → close the modal and call `loadMeals()` to refresh the overview list.
  - `created.length >= 1 && failed.length > 0` → follow the single-vs-multiple rule above for the created meals (if exactly one created, navigate to detail; if multiple, close + refresh list), AND display the failures inline per FR-005.
  - `created.length === 0 && failed.length > 0` → keep the modal open, display the failures inline, and do not navigate.
- **FR-005**: When `failed.length > 0`, the modal MUST show a results block listing each failed URL with its reason string. The existing i18n mapping for reasons (`importBulkReasonFetch`, `importBulkReasonNoRecipe`, `importBulkReasonValidation`) is reused. A "New batch" action clears the textarea and results, allowing the user to retry.
- **FR-006**: The Import button MUST be disabled when the textarea is empty (no non-empty lines) or when an import is in progress. The button label shows the loading state (e.g. "Importing…") while the request is outstanding.
- **FR-007**: The modal MUST show a clear validation error when the textarea contains more than 50 non-empty URLs before any API call is made, reusing the existing `importBulkErrorMaxUrls` i18n key.
- **FR-008**: Raw HTML/JSON-LD paste input MUST be removed from the add-meal modal. The "Import URLs" textarea accepts recipe URLs only. The `/api/import/paste` endpoint is no longer called from the modal (the endpoint itself remains available for other internal callers but is out of scope for this spec).
- **FR-009**: The draft→review→collapse state machine (`importCollapsed`, `importInput`, `importInput.trim()` URL-vs-paste dispatch, `importButtonUseDraft`, `importCollapsedSummary`, `importCollapsedExpand`) MUST be removed from the URL import path. The `MealForm` panel is no longer shown alongside the URL import tab (it remains shown alongside the LLM tab and for the manual entry when no import tab is active, per existing layout).
- **FR-010**: The LLM import tab (`importMode === 'llm'`) MUST remain unchanged: provider/model selector, hint textarea, `importFromLlm` call returning an `ImportDraft`, draft populating the `MealForm`, explicit save via `onSubmitAdd`.
- **FR-011**: The manual meal-entry form MUST remain available. When the user fills the form and submits without using any import, `createMeal` is called, the modal closes, and `loadMeals()` refreshes the list (existing behavior).
- **FR-012**: All existing backend API contracts (`/api/meals`, `/api/meals/:id`, `/api/import/bulk`, `/api/import/url`, `/api/import/paste`, `/api/import/llm`, `/api/import/image-url`) MUST remain unchanged in contract and behavior. No backend code changes are required.
- **FR-013**: i18n keys for the removed "From link" flow (`importTabLink`, `importLinkLabel`, `importLinkPlaceholder`, `importButtonFetch`, `importButtonUseDraft`, `importCollapsedSummary`, `importCollapsedExpand`) MUST be removed from both `en.ts` and `de.ts`. The `importTabBulk` key is renamed to a unified tab label (e.g. `importTabUrls`). German translations are updated to match.

## Edge Cases
- **Empty textarea**: Import button disabled (FR-006); no API call.
- **Whitespace-only or blank lines**: Ignored during URL count and submission (existing `trim()` + `filter` behavior preserved).
- **51+ non-empty URLs**: Client-side validation error `importBulkErrorMaxUrls`; no API call (FR-007).
- **Non-URL input** (no `http://`/`https://` prefix): Client-side validation error before any API call (FR-002). The previous auto-dispatch to `/api/import/paste` is removed.
- **Exactly one created, zero failures**: Navigate to `/meals/<id>` detail view (FR-004).
- **Exactly one created, with failures**: Navigate to `/meals/<id>` detail view for the created meal; failures are not shown (the modal is closed by the navigation).
- **Multiple created, with failures**: Close modal, refresh overview list; failures are shown inline before close.
- **Zero created, all fail**: Keep modal open, show failures (FR-004, FR-005).
- **Network error on bulk import request**: Inline error in modal (existing `importErrorFetch` / `ApiError` handling), modal stays open.
- **Stale form state after a successful import**: `openAdd()` resets `bulkUrls`, `bulkResult`, `bulkError` (existing behavior in `openAdd`).
- **LLM tab unaffected**: The `importCollapsed`/draft machinery is removed from the URL path but the LLM path's draft→form flow continues to work via its own `onImport` branch (FR-010).

## Assumptions
- The existing `POST /api/import/bulk` endpoint and `importBulk()` frontend function support 1–50 URLs and return `BulkImportResult { created: Meal[], failed: BulkImportFailure[] }`; no backend change is needed.
- The meal detail view at `/meals/<id>` (`web/src/routes/meals/[id]/+page.svelte`) already exists and renders a created meal correctly; navigating to it via `goto('/meals/<id>')` requires no new route.
- "From link" raw HTML/JSON-LD paste support is fully removed from the add-meal modal (user confirmed URLs only). The `/api/import/paste` endpoint remains available for any internal caller but is no longer invoked from the modal.
- The `importCollapsed` draft→review state machine (import-field-collapse UI for "From link") is removed entirely from the URL import path; the LLM tab's draft flow uses its own `onImport` branch and is preserved.
- The existing `MealForm` panel remains shown alongside the LLM import tab (two-panel layout) and for manual entry; the URL import tab is single-panel (no `MealForm` alongside).
- German translations will mirror the English key changes (verified — `de.ts` mirrors `en.ts`).
- E2E tests `import-meal.spec.ts` and `import-paste.spec.ts` will be rewritten/removed to reflect direct-persist navigation; `import-bulk.spec.ts` will be updated for the unified tab label and single-URL→detail navigation case.

## Success Criteria
- **SC-001**: Pasting exactly one valid recipe URL into the unified "Import URLs" textarea and clicking Import persists the meal directly and navigates the user to `/meals/<id>` (the meal detail view) with no intermediate draft/review form.
- **SC-002**: Pasting two or more valid recipe URLs (one per line, ≤50) into the unified textarea and clicking Import persists all meals directly, closes the modal, and refreshes the overview list with the new meals visible.
- **SC-003**: When a bulk import produces failures, the modal shows the success count and a per-URL failure list with reason strings (`fetch failed`, `no recipe found`, `validation failed`); when some meals were created the list is refreshed; when exactly one meal was created the user is navigated to its detail view.
- **SC-004**: The "From link" tab, raw HTML/JSON-LD paste input, draft→review collapse UI, and their associated i18n keys are removed. The add-meal modal shows exactly "Import URLs" and "AI import" tabs.
- **SC-005**: The LLM import tab continues to populate the manual form with a parsed draft and requires an explicit save (unchanged behavior).
- **SC-006**: The manual meal-entry form is still available and persists a meal on submit without any import (unchanged behavior).
- **SC-007**: The Import button is disabled when the textarea is empty or an import is in progress, and shows a loading label during import.
- **SC-008**: More than 50 non-empty URLs in the textarea produces a client-side validation error and makes no API call.
- **SC-009**: All backend API contracts remain unchanged; no backend code is modified.
- **SC-010**: All existing Rust tests (`cargo test`) continue to pass unchanged.
- **SC-011**: Updated Vitest and Playwright E2E tests pass (import-meal.spec.ts rewritten for direct-persist + detail navigation; import-paste.spec.ts removed; import-bulk.spec.ts updated for unified tab).
