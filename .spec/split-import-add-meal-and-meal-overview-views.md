# Feature Specification: Add-Meal Dialog on Meal Overview

**Created**: 2026-06-28
**Status**: Approved
**Input**: Split import / add meal and meal overview views (v2: dialog approach supersedes route-based split)

## Goal
The `/meals` overview provides an add-meal modal dialog (opened by clicking the "Add meal" button) containing import tabs (URL, paste, LLM) and a manual meal entry form. On successful create or import the dialog closes and the overview list refreshes in place — no route change, no redirect. The edit flow remains a modal on the same overview, giving each concern its own surface without extra routes.

## User Scenarios

### Scenario 1 - Browse the meal collection (P1)
The user opens the app, clicks "Meals" in the navigation, and lands on the meal overview. They see their full meal list, can type in the search bar to filter by name or ingredients, click a meal image to open the lightbox, and delete a meal with a confirmation dialog.

**Acceptance**
1. Given a user with meals in their collection, when they navigate to `/meals`, then they see the full meal list with a search bar.
2. Given the meal overview is displayed, when the user types a search term, then the list filters server-side to matching meals.
3. Given a meal with an image in the list, when the user clicks its thumbnail, then a lightbox opens showing the full image.
4. Given a meal in the list, when the user clicks its delete button and confirms, then the meal is removed and the list updates.

### Scenario 2 - Add a new meal (P1)
The user is on the meal overview and clicks the "Add meal" button. An add-meal dialog opens on `/meals` showing the import tabs (URL, paste, LLM) and the manual entry form. They fill in name and ingredients, optionally upload an image, and submit. On success the dialog closes and the overview list updates with the new meal visible.

**Acceptance**
1. Given the user is on `/meals`, when they click the "Add meal" button, then an add-meal dialog opens on `/meals`.
2. Given the add dialog is open, when they fill in a valid name and ingredients and submit the manual form, then the meal is created, the dialog closes, and the new meal appears in the overview list (no redirect).
3. Given the add dialog is open, when they switch to the URL import tab, paste a valid recipe URL, and submit, then the meal is imported, the dialog closes, and the imported meal appears in the overview list.
4. Given the add dialog is open, when import fails (e.g., unreachable URL), then an error message appears inline in the dialog and it stays open to retry.

### Scenario 3 - Edit a meal from the overview (P2)
The user is browsing the meal overview, finds a meal they want to update, and clicks its edit button. A modal overlay opens pre-filled with the meal's current name, ingredients, and image. They modify the fields, save, and the modal closes with the updated meal reflected in the list.

**Acceptance**
1. Given a meal in the overview list, when the user clicks its edit button, then a modal overlay opens showing the meal's current name, ingredients, instructions, and existing image (if any).
2. Given the edit modal is open, when the user modifies fields and clicks save, then the meal is updated, the modal closes, and the list reflects the changes.
3. Given the edit modal is open, when the user clicks cancel or presses Escape, then the modal closes and no changes are persisted.
4. Given the edit modal is open while a search filter is active, when the user saves or cancels, then the search state is preserved and the filtered list is shown.

### Scenario 4 - Add dialog open/close behavior (P1)
The add dialog is opened and closed in place on `/meals`. Escape key and backdrop click close the dialog. No route change occurs.

**Acceptance**
1. Given the user is on `/meals`, when they click the "Add meal" button, then the add dialog opens in place (URL bar stays at `/meals`).
2. Given the add dialog is open, when the user presses Escape or clicks the backdrop, then the dialog closes and the overview is unchanged.
3. Given the add dialog is open, when the user clicks the X close button, then the dialog closes.

## Functional Requirements

- **FR-001**: The `/meals` route MUST render the meal overview, containing: the meal list, a server-side search bar, per-meal edit/delete actions, an image lightbox, and the delete confirmation dialog.
- **FR-002**: The `/meals` overview MUST provide an add-meal dialog (opened via the "Add meal" button) containing: import tabs (URL import, paste import, LLM import) and the manual meal entry form (name, ingredients, instructions, image upload).
- **FR-003**: The overview MUST include an "Add meal" button that opens the add dialog; closing the dialog returns to the overview (no navigation).
- **FR-004**: Editing a meal MUST open a modal overlay on the overview page, pre-filled with the meal's existing data (name, ingredients, instructions, image). Saving updates the meal via the existing API; canceling or pressing Escape dismisses the modal without changes.
- **FR-005**: After a successful meal creation or import, the dialog MUST close and the new meal MUST appear in the overview list (no navigation or redirect).
- **FR-006**: The search bar and its filtering behavior MUST live on the overview view only; the add dialog has no search.
- **FR-007**: Deleting a meal from the overview MUST use the existing `DeleteConfirmDialog` pattern (two-step: click delete, then confirm).
- **FR-008**: Navigating to `/meals` MUST land on the overview view by default.
- **FR-009**: All existing API endpoints (`/api/meals`, `/api/meals/:id`, `/api/import/*`) MUST remain unchanged in contract and behavior.
- **FR-010**: The existing `/meals/[id]` cooking/detail view MUST be unaffected.
- **FR-011**: Import errors (URL unreachable, paste unparseable, LLM failure) MUST display inline in the open add dialog without navigating away, allowing the user to correct and retry.
- **FR-012**: The edit modal MUST preserve any active search filter on the overview when it opens and closes.

## Edge Cases

- **Add dialog while search is active**: Closing the add dialog preserves the active search term and filtered list (parallel to the existing edit-modal behavior).
- **Stale deep-link to `/meals/add`**: The SPA fallback renders the `/meals` overview (the old URL is internal, not externally advertised).
- **Empty meal collection**: The overview shows an appropriate empty state when no meals exist.
- **Edit while search is active**: The modal overlays the filtered list; on close the search term and filtered results are preserved.
- **Concurrent edits**: Last-write-wins behavior (existing API behavior, unchanged).
- **Image lightbox on overview**: Works identically to current behavior — click thumbnail to open, Escape or click backdrop to close.
- **Import while form has unsaved data**: Switching between import tabs or navigating away does not preserve unsaved manual form input (no draft-saving).

## Assumptions

- SvelteKit's adapter-static with hash-based routing (`fallback: 'index.html'`) continues to serve the `/meals` overview for any path not matching a SvelteKit route, including the now-removed `/meals/add`.
- All existing API contracts (`/api/meals`, `/api/meals/:id`, `/api/import/*`) remain unchanged — this is a frontend-only restructuring.
- The existing `DeleteConfirmDialog` component is reused as-is.
- The `/meals/[id]` cooking/detail view is out of scope and requires no changes.
- No new backend endpoints, DB schema changes, or Rust code changes are needed.
- The current styling conventions (plain CSS, system-ui font, glass headers) apply to all new UI elements.

## Success Criteria

- **SC-001**: Navigating to `/meals` displays the meal overview with search, list, and per-meal actions.
- **SC-002**: Opening the add-meal dialog on `/meals` displays import tabs and a manual form.
- **SC-003**: Creating a meal via the manual form in the add dialog succeeds, the dialog closes, and the new meal is visible in the overview (no redirect).
- **SC-004**: Importing a meal via URL in the add dialog succeeds, the dialog closes, and the imported meal is visible in the overview (no redirect).
- **SC-005**: Editing a meal from the overview opens a pre-filled modal; saving updates the meal and closes the modal; canceling discards changes.
- **SC-006**: Deleting a meal from the overview shows a confirmation dialog; confirming removes the meal from the list.
- **SC-007**: Search on the overview filters the meal list server-side and is unaffected by opening/closing the edit modal.
- **SC-008**: All existing frontend tests (Vitest) continue to pass after the restructuring.
- **SC-009**: All existing Rust tests (`cargo test`) continue to pass with zero changes.
- **SC-010**: The existing `/meals/[id]` cooking view continues to function exactly as before.

## Research Notes

- The dialog approach (2026-06-28) supersedes the original route-based split: hash-routing deep-link to `/meals/add` is no longer needed. Import + manual form live in a modal on `/meals`; the close flow uses `loadMeals()` + `closeAdd()` instead of `goto('/meals')`.
