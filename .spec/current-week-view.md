# Feature Specification: Current-Week View as Conditional Landing Page

**Created**: 2026-06-22
**Status**: Approved
**Input**: New simple view: current week — shows meals of the currently planned week, default landing page if the week is planned, if week is not planned planner is shown.

## Goal
Add a "current week" view that shows the meals and ingredient summary of the currently planned calendar week. Make `/` land on this view when a plan exists for the current week; otherwise land on the planner with the current week auto-opened. Move the existing meal manager from `/` to `/meals`, and add a read-only "cooking view" route at `/meals/:id` for viewing a single meal's full details (image, ingredients, quantities) while cooking.

## User Scenarios
### Scenario 1 - Planned week lands on current-week view (P1)
A user opens the app and the current calendar week has a plan. The landing page shows the current week's date range, the list of planned meals, and the aggregated ingredient summary. Each meal links to its cooking view.

**Acceptance**
1. Given the current week has a plan (getPlan(year, week) returns non-null), When the user navigates to `/`, Then the current-week view renders the date range, planned meals list, and ingredient summary, not the meal manager or planner.
2. Given the current-week view is rendered, When the user activates a meal in the list, Then the browser navigates to `/meals/:id` for that meal.

### Scenario 2 - Unplanned week lands on planner with current week auto-opened (P1)
A user opens the app and the current calendar week has no plan. The landing page is the planner, and the current week's detail panel is automatically open and ready to generate a plan.

**Acceptance**
1. Given the current week has no plan (getPlan returns null), When the user navigates to `/`, Then the planner view is rendered as the landing.
2. Given the planner renders as the landing due to an unplanned current week, When the view finishes loading, Then the current week is auto-selected and its detail panel is open (showing the "generate plan" affordance).

### Scenario 3 - Cooking view at /meals/:id (P1)
A user opens a meal's cooking view to see its full read-only details while cooking. The view shows the image, name, and full ingredient list with quantities. No edit or delete affordance is present on this route.

**Acceptance**
1. Given a meal with id N exists, When the user navigates to `/meals/N`, Then a read-only view renders the meal's image (when present), name, and all ingredients with their quantities.
2. Given the user is on `/meals/:id`, When the user scans the view, Then no create/edit/delete controls are rendered on this route.
3. Given no meal exists for id N, When the user navigates to `/meals/N`, Then the view shows a "not found" state.

### Scenario 4 - Meal manager relocated to /meals (P2)
A user manages meals (create, edit, delete, search, image upload) at `/meals`, reusing the existing meal manager UI that previously lived at `/`.

**Acceptance**
1. Given the app is running, When the user navigates to `/meals`, Then the existing meal manager UI (list, search, create/edit form, delete, image lightbox) renders unchanged in behavior.
2. Given any existing internal navigation previously pointing to `/` for meal management, When the routes are updated, Then those links point to `/meals`.

## Functional Requirements
- **FR-001**: The `/` route computes the current week using the existing `weekOfDate(new Date())` helper and queries `getPlan(year, week)`.
- **FR-002**: When `getPlan` returns non-null, `/` renders a current-week view showing: the week's date range (via existing `mondaySundayOf`), the planned meals list, and the `ingredient_summary`.
- **FR-003**: When `getPlan` returns null, `/` renders the planner with the current week auto-selected (detail panel open, ready to generate).
- **FR-004**: Each meal in the current-week view links to `/meals/:id`.
- **FR-005**: A new `/meals/:id` route renders a read-only cooking view: meal image (when `has_image`), name, and full ingredient list with quantities. No edit/delete/create controls on this route.
- **FR-006**: A meal id with no matching record on `/meals/:id` renders a "not found" state.
- **FR-007**: The existing meal manager UI is served from `/meals` (relocated from `/`), preserving all current behavior (list, search, create, edit, delete, image lightbox).
- **FR-008**: All existing internal navigation referencing `/` for meal management is updated to `/meals`.
- **FR-009**: A plan with zero meals is considered "planned" (landing shows the current-week view with an empty-meals state), not unplanned.
- **FR-010**: If `getPlan` fails (network/API error) during landing resolution, `/` shows a graceful error state with a retry affordance rather than a blank page or crash.

## Key Entities
- **Plan**: existing entity keyed by `(year, week_number)` with `meals[]` and `ingredient_summary[]`. Reused as-is; no schema change.
- **Meal**: existing entity. Reused as-is; `/meals/:id` reads `GET /api/meals/:id`.
- **Week**: computed via existing `weekOfDate`/`mondaySundayOf` helpers; no new week logic.
- **CurrentWeekInfo**: `{ year, week }` derived from `weekOfDate(new Date())`; drives the landing decision and planner auto-select.

## Edge Cases
- Year-boundary dates (late December / early January) resolve to the correct year+week via the existing `weekOfDate` recursion; the landing decision uses that result unchanged.
- A plan that exists with zero meals is treated as planned; the current-week view shows an empty-meals state (not the planner).
- Network or API failure when resolving the landing plan renders a graceful error state with retry, not a blank page.
- Navigating to `/meals/:id` for a deleted or non-existent id renders a "not found" state (consistent with a 404 from `GET /api/meals/:id`).
- "Current week" is computed in browser time using the existing `weekOfDate`; users in different timezones may see a different current week near midnight UTC boundaries — this matches existing planner behavior and is not changed by this feature.
- Deep-linking to `/`, `/meals`, `/meals/:id`, and `/planner` works directly (each route resolves independently of navigation history).

## Assumptions
- The cooking view at `/meals/:id` is read-only; meal editing continues to happen on `/meals` via the existing inline-on-list edit flow. No edit affordance is added to `/meals/:id`.
- No backend changes are required: `GET /api/meals/:id` already exists, `GET /api/plans/:year/:week` already exists, and all data needed by the new routes is already served by these endpoints.
- "Auto-open current week" on the planner reuses the planner's existing detail-panel logic; no new planner features beyond auto-selecting the current week on load.
- The current-week view reuses `Plan.ingredient_summary` and `Meal` fields as-is; no new aggregation logic.
- Existing styling conventions (single `app.css`, plain CSS, no UI framework) apply to the new routes.
- SvelteKit routing conventions (Svelte 5 runes, `+page.svelte` per route) are used; no new state library.

## Success Criteria
- **SC-001**: Navigating to `/` when the current week has a plan renders the current-week view (date range, meals, ingredient summary); it does not render the meal manager or the planner.
- **SC-002**: Navigating to `/` when the current week has no plan renders the planner with the current week auto-selected and its detail panel open.
- **SC-003**: Navigating to `/meals/:id` for an existing meal renders a read-only view with image (when present), name, and all ingredients with quantities, and contains no edit/delete/create controls.
- **SC-004**: Navigating to `/meals/:id` for a non-existent id renders a "not found" state.
- **SC-005**: Navigating to `/meals` renders the full meal manager (list, search, create, edit, delete, image lightbox) with behavior unchanged from the previous `/` meal manager.
- **SC-006**: A plan with zero meals is treated as planned: `/` renders the current-week view with an empty-meals state, not the planner.
- **SC-007**: A `getPlan` failure on `/` renders a graceful error state with a retry affordance, not a blank page or uncaught error.
- **SC-008**: `cargo test` and `cd web && npm test` pass with no regressions after the change.
