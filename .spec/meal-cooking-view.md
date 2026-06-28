# Feature Specification: Meal Card Navigation to Cooking View

**Created**: 2026-06-28
**Status**: Approved
**Input**: when i click on a meal on the meal overview i want to open the meal to be able to cook it

## Goal
On the meals overview page (`/meals`), meal cards currently have no path to the cooking view — the name is plain text, the thumbnail opens a lightbox, and only edit/delete actions are available. Users need a direct, one-click way to open a meal for cooking. The existing cooking view at `/meals/[id]` already displays name, image, ingredients with quantities, and instructions in a cooking-optimized layout. This feature adds navigation from the meal overview cards to that view without altering the cooking view itself or regressing existing card interactions.

## User Scenarios
### Scenario 1 - Navigate from meal card to cooking view (P1)
A user browsing all meals on the `/meals` page sees a meal they want to cook. They click the meal card and are taken directly to the cooking view showing that meal's ingredients and instructions.

**Acceptance**
1. Given a populated meal list on `/meals`, when the user clicks (or taps) anywhere on a meal card that is not the thumbnail or an action button, then the browser navigates to `/meals/<id>` and the cooking view loads with that meal's full details.
2. Given a meal card on `/meals`, when a keyboard user tabs to the card and presses Enter or Space, then navigation to the cooking view occurs.
3. Given a meal card on `/meals`, when a screen reader encounters the card, then it announces the card as a navigable link to the cooking view for that meal.

### Scenario 2 - Thumbnail lightbox preserved (P2)
A user wants to see a meal's image full-size before deciding to cook it. They click the thumbnail on the meal card and the lightbox opens as before.

**Acceptance**
1. Given a meal card with an image on `/meals`, when the user clicks the thumbnail image, then the lightbox opens showing the full-size meal image and no navigation occurs.

### Scenario 3 - Edit and delete actions preserved (P3)
A user wants to edit or delete a meal from the overview. They click the edit or delete button on the card and the respective modal opens as before.

**Acceptance**
1. Given a meal card on `/meals`, when the user clicks the Edit button, then the edit modal opens for that meal and no navigation occurs.
2. Given a meal card on `/meals`, when the user clicks the Delete button, then the delete confirmation dialog opens for that meal and no navigation occurs.

## Functional Requirements
- **FR-001**: Clicking or tapping a meal card on `/meals` anywhere outside the thumbnail image and action buttons (edit, delete) MUST navigate the browser to `/meals/<id>`.
- **FR-002**: Clicking or tapping the meal card thumbnail MUST open the existing image lightbox and MUST NOT trigger navigation.
- **FR-003**: Clicking or tapping the Edit button on a meal card MUST open the existing edit modal and MUST NOT trigger navigation.
- **FR-004**: Clicking or tapping the Delete button on a meal card MUST open the existing delete confirmation dialog and MUST NOT trigger navigation.
- **FR-005**: The meal card MUST be keyboard-focusable and activatable via Enter and Space keys to trigger navigation.
- **FR-006**: The meal card MUST expose an accessible role (link or button) and an accessible name that identifies the meal to screen readers.
- **FR-007**: Event propagation from thumbnail and action button clicks MUST be stopped so they do not bubble up and trigger card-level navigation.

## Edge Cases
- Clicking the empty/gap area between the thumbnail, name, ingredient preview, and action buttons must count as a card click and navigate.
- Rapid double-click on the card must not cause duplicate navigation or errors.
- On narrow/mobile viewports, the edit and delete buttons must have sufficient tap target size (minimum 44×44 CSS pixels) to prevent accidental navigation when the user intends to edit or delete.
- Meals without images have no thumbnail and therefore no lightbox trigger; the entire card surface (except action buttons) navigates.
- The current-week overview page (`/`) already links meals to `/meals/<id>` via `<a>` elements — no change is needed there.
- If navigation to `/meals/<id>` results in a 404 (meal deleted between page load and click), the cooking view's existing not-found state handles it.

## Assumptions
- The existing cooking view at `/meals/[id]` is sufficient and requires no enhancements as part of this feature.
- "Meal overview" refers to the `/meals` page (the all-meals browsing page), not the current-week dashboard.
- The image lightbox on the meals overview page must be preserved alongside the new navigation behavior.
- No backend or API changes are required.

## Success Criteria
- **SC-001**: Clicking a meal card (excluding thumbnail and action buttons) navigates to `/meals/<id>` in under 500 ms on a local connection.
- **SC-002**: Thumbnail click still opens the lightbox without triggering navigation.
- **SC-003**: Edit and delete button clicks still open their respective modals without triggering navigation.
- **SC-004**: All existing tests on the `/meals` page and cooking view continue to pass.
- **SC-005**: Keyboard-only users can navigate meal cards with Tab, Enter, and Space.
