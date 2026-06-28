# Feature Specification: Recipe Import from URL

**Created**: 2026-06-22
**Status**: Approved
**Input**: Add meal/recipe import like in Mealie (or even better)

## Goal
Let users add a complete recipe to MealMe by pasting a recipe URL (or raw HTML/JSON-LD when a site blocks server-side fetches). The backend scrapes schema.org Recipe structured data and returns it as a reviewable meal draft, skipping manual entry. The project adds an instructions field to the Meal entity to capture the full recipe.

## User Scenarios
### Scenario 1 - Import from URL (P1)
A user finds a recipe on a cooking website and wants it in MealMe without retyping every ingredient. They open the import UI, paste the recipe page URL, and MealMe fetches and parses the page. The scraped name, ingredients, instructions, and image populate the existing add/edit form for review; the user adjusts anything that looks wrong and saves.

**Acceptance**
1. Given a recipe page that publishes schema.org Recipe (JSON-LD or Microdata), When the user submits its URL to the import endpoint, Then the backend fetches the page, parses the Recipe structured data, and returns a meal draft containing name, ingredients (as IngredientQuantity[]), instructions (text), and an image (when present and reachable).
2. Given the backend returns a parsed meal draft, When it is received by the frontend, Then the draft populates the existing add/edit form fields (name, ingredient rows, instructions field, image) and the meal is NOT persisted until the user explicitly saves.
3. Given a recipe whose recipeInstructions is an array of HowToStep/HowToSection objects (not plain text), When parsed, Then the steps are joined into a single instructions text field in the draft.

### Scenario 2 - Import from pasted raw HTML / JSON-LD (P2)
A user hits a paywalled or bot-blocking site where a server-side fetch fails. They open the page source in their browser, copy the HTML (or the JSON-LD block), and paste it into MealMe's import textarea. MealMe parses it with the same logic as Scenario 1, without performing any network fetch.

**Acceptance**
1. Given the user selects "paste HTML/JSON-LD" mode and submits raw page HTML containing schema.org Recipe markup, When processed, Then the backend parses the Recipe data from the pasted HTML and returns the same meal draft shape as Scenario 1, without making an outbound network request for the page.
2. Given the user pastes a raw JSON-LD object (an `application/ld+json` script body) with `@type` `Recipe`, When processed, Then the backend parses it directly and returns the meal draft.

### Scenario 3 - Image auto-import (P1)
The scraped recipe includes an image URL in its structured data. MealMe downloads the image and runs it through the existing image pipeline so the resulting meal record has an image attached, matching the experience of a manually entered meal with a photo.

**Acceptance**
1. Given a parsed Recipe with an `image` field containing a reachable URL, When the import succeeds, Then the downloaded image is passed through the existing image processing pipeline (src/image.rs: any format → JPEG, downscale max 3840px, quality 82) and attached to the draft as the meal image.
2. Given a parsed Recipe whose image URL is missing, unreachable, or returns an invalid image, When the import otherwise succeeds, Then the meal draft is still returned with all other fields populated and the image left blank; the image failure must not fail the import.

### Scenario 4 - Scrape failure with structured error (P1)
A user submits a URL whose page contains no schema.org Recipe, or the fetch is blocked (403), times out, or the page returns malformed markup. MealMe does not create a partial meal; it returns a structured error that tells the user what went wrong and suggests they retry with another URL or switch to paste mode.

**Acceptance**
1. Given a URL whose fetched page contains no schema.org Recipe (JSON-LD or Microdata), When the import is attempted, Then the backend returns a structured error response (HTTP 4xx, body `{"error":"..."}`) with an actionable message naming the failure reason, and no meal is persisted.
2. Given a URL whose fetch fails (non-2xx status, connection error, or timeout), When the import is attempted, Then the backend returns a structured error response naming the cause, and no meal is persisted.
3. Given a URL whose fetched page contains malformed JSON-LD that cannot be parsed, When the import is attempted, Then the backend returns a structured error response naming the parse failure, and no meal is persisted.

## Functional Requirements
- **FR-001**: Provide an import endpoint that accepts a source URL as input; the backend fetches the page server-side (no client-side fetching) and parses schema.org Recipe structured data, preferring JSON-LD and falling back to Microdata.
- **FR-002**: Provide an import path that accepts raw pasted HTML (page source) or raw JSON-LD as input; the backend parses Recipe structured data using the same logic as FR-001 and makes no outbound network request for the page.
- **FR-003**: Map the extracted Recipe fields into the existing Meal structures: `name` → Meal.name; `recipeIngredient` → IngredientQuantity[] with a best-effort name/quantity split (non-splittable lines become name with empty quantity); `recipeInstructions` → Meal.instructions (HowToStep/HowToSection arrays joined into plain text).
- **FR-004**: Add an `instructions` field to the Meal entity: a TEXT column via a database migration, surfaced through the CRUD layer, serde DTOs (`Meal`, `NewMeal`, `MealPatch` or equivalents), API request/response bodies, frontend `types.ts`, the meal form, and the meal detail view. Enforce validation (max length, non-empty rules for save) on both backend and frontend, mirroring the existing name/ingredients validation pattern.
- **FR-005**: When the parsed Recipe contains an `image` field with a reachable URL, download the image server-side and pass it through the existing image processing pipeline (`src/image.rs`: any input format → JPEG, downscale max 3840px long edge, quality 82) so the resulting draft carries the image as a BLOB, exactly like a manually uploaded meal image.
- **FR-006**: Enforce review-first UX: the import result is a meal draft that populates the existing add/edit form. The user must explicitly save to persist. Import never writes directly to the database.
- **FR-007**: On any scrape failure (no schema.org Recipe found, network error, non-2xx HTTP status, timeout, malformed markup), return a structured error response matching the existing `AppError` → `{"error":"..."}` pattern with an actionable message that names the failure reason and suggests the retry/paste fallback. Never persist a partial meal.
- **FR-008**: All server-side HTTP fetching must use a rustls-based client (e.g., `reqwest` with the `rustls-tls` feature). No `native-tls`, no OpenSSL, per the project TL  rule.
- **FR-009**: Bound every server-side fetch with a finite connect+read timeout and a maximum response body size to prevent runaway downloads; both are configured and documented at the implementation layer.
- **FR-010**: Treat all parsed content as untrusted. Names, ingredient lines, and instructions must be rendered safely in the UI (no raw HTML injection) and stored as plain text in the database.

## Key Entities
- **Meal**: gains an `instructions` field (TEXT). Existing fields unchanged: `id`, `name`, `ingredients` (Vec<IngredientQuantity>), `last_planned_at`, `created_at`, `updated_at`, `has_image`/`image` BLOB.
- **IngredientQuantity**: unchanged (name + optional quantity). Import maps `recipeIngredient` strings into this shape best-effort.
- **ImportRequest**: the input to the import endpoint(s) — either a source URL (FR-001) or raw HTML/JSON-LD payload (FR-002).
- **ImportDraft**: the parsed output returned to the frontend — a meal-shaped object (name, ingredients[], instructions, optional image) suitable for populating the add/edit form. Not a persisted Meal until the user saves.

## Edge Cases
- Pages without any schema.org Recipe (JSON-LD or Microdata) → structured error; no partial import.
- Bot-blocking (HTTP 403), paywalled, or rate-limited sites → structured error; user falls back to paste mode.
- `recipeInstructions` provided as plain text vs. a `HowToStep[]` vs. a `HowToSection[]` → all handled; steps are joined into a single instructions text field, preserving order.
- `recipeIngredient` entries that do not split cleanly into name+quantity → whole line stored as Ingredient name with empty quantity.
- Multiple Recipe JSON-LD objects on one page → parse the first object whose `@type` is `Recipe`.
- Malformed JSON-LD (invalid JSON, missing required fields) → structured parse error; user retries.
- Fetch timeout or network error → structured error response naming the cause.
- Image URL missing, unreachable, or not a valid image → non-fatal; proceed with image left blank, all other fields still populated.
- Pasted input is neither valid HTML-with-Recipe nor a valid JSON-LD Recipe → structured error.
- Untrusted parsed content (names, ingredients, instructions) must not carry through into rendered HTML unescaped; no `<script>` injection path.
- Server-side fetch bounded by finite timeout and max response body size to prevent runaway downloads.

## Research Notes
- https://schema.org/Recipe — the structured-data standard MealMe parses. Key fields: `name`, `recipeIngredient` (ItemList/PropertyValue/Text), `recipeInstructions` (CreativeWork/ItemList/Text, including HowToStep/HowToSection), `image` (URL/ImageObject), `recipeYield`. `recipeInstructions` may be a single text blob or a nested HowToStep[]/HowToSection[] array, which is why FR-003 requires joining logic.
- https://github.com/hhursev/recipe-scrapers — Mealie uses this Python library, which combines per-site scrapers with a schema.org fallback. MealMe intentionally takes a schema.org-first approach (no per-site scrapers) for simplicity and lower maintenance. This covers the majority of mainstream recipe sites that publish schema.org markup. Per-site heuristic scraping is out of scope.
- https://docs.mealie.io/documentation/getting-started/features/ — documents Mealie's two import paths (URL scraper + "Recipe HTML or JSON" paste). This spec mirrors that two-path design so users have a fallback when a site blocks server-side fetches.
- https://crates.io/crates/recipe-scraper and https://docs.rs/reget/latest/reget/ — existing Rust crates that parse schema.org Recipe from HTML into a struct (JSON-LD-focused). Alternatively, because MealMe already depends on `serde_json`, a thin serde-based parser over extracted JSON-LD script bodies is viable. The choice between an off-the-shelf crate and a thin custom parser is a `/plan` decision, not a `/specify` decision; both satisfy FR-001/FR-002.

## Assumptions
- `Meal.instructions` is a TEXT column on the `meals` table with a max length validated on both backend and frontend; the exact limit (recommendation: ≤ 20000 characters) is finalized at plan time.
- The new HTTP client dependency uses rustls (e.g., `reqwest` with the `rustls-tls` feature), never `native-tls`/OpenSSL, per the project TLS rule.
- Server-side fetch uses a finite timeout (recommendation: 30s connect+read) and a maximum response body size (recommendation: 2MB); exact values are finalized at plan time.
- schema.org Recipe (JSON-LD and Microdata-embedded) is the only supported input shape. Prose-only recipes without structured data yield a structured error. Heuristic and per-site scraping are explicitly out of scope.
- Ingredients are parsed best-effort into the existing IngredientQuantity shape; lines that do not split into name+quantity become the ingredient name with an empty quantity.
- Image fetch failure during import never blocks the recipe import itself; the meal draft is still returned with all other fields populated.
- No silent field drops: every field that fails to parse is left empty in the draft for the user to fill during review.
- The frontend never performs outbound fetches to external recipe URLs; all fetching is server-side through MealMe.

## Success Criteria
- **SC-001**: A user can paste a recipe URL for a page that publishes schema.org Recipe (JSON-LD) and receive a meal draft with name, ingredients, instructions, and (when present and reachable) image populated correctly for review, without typing any field manually.
- **SC-002**: A user can paste raw page HTML (or a raw JSON-LD block) from a site that blocks server-side fetching and receive the same meal draft shape, with no outbound network request made for the page.
- **SC-003**: The `Meal` entity has an `instructions` field persisted in the database, returned by all CRUD endpoints, validated on backend and frontend, and editable in the meal form and visible in the meal detail view.
- **SC-004**: When a recipe includes a reachable image URL, importing produces a meal draft whose image is stored through the same pipeline as a manual upload (BLOB, JPEG quality 82, max 3840px long edge), and an unreachable image does not fail the import.
- **SC-005**: When a URL is submitted whose page has no schema.org Recipe, or when the fetch fails (non-2xx, timeout, malformed markup), the API returns a structured `{"error":"..."}` response with an actionable message and no meal is persisted.
- **SC-006**: The import result is never written directly to the database; it always populates the add/edit form and requires an explicit user save to persist.
