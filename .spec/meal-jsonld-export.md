# Feature Specification: Meal JSON-LD Export

**Created**: 2026-06-29
**Status**: Approved
**Input**: Export meals as schema.org Recipe JSON-LD so other systems can import MealMe meals

## Goal
MealMe already consumes schema.org Recipe JSON-LD for importing meals from recipe websites. This feature adds the reverse direction: exposing MealMe meals as standards-compliant JSON-LD so external recipe managers, crawlers, and tools can programmatically read them. The feature is API-only (no HTML embedding), covers both single-meal and bulk retrieval, and emits only the fields MealMe actually stores.

## User Scenarios
### Scenario 1 - Export a single meal as JSON-LD (P1)
A user wants to share one meal with another recipe application that understands schema.org Recipe. They construct a request for the meal by ID with an appropriate Accept header, receive standards-compliant JSON-LD, and feed it to the target system.

**Acceptance**
1. Given a meal with id=42 exists, when `GET /api/meals/42` is called with `Accept: application/ld+json`, then the response has `Content-Type: application/ld+json` and the body is a valid schema.org `Recipe` JSON-LD object containing `@context`, `@type: "Recipe"`, `name`, `recipeIngredient`, `recipeInstructions`, `datePublished`, and `dateModified`.
2. Given a meal with id=42 exists and has an image (`has_image: true`), when the JSON-LD response is examined, then the `image` property is an absolute URL (e.g. `http://127.0.0.1:11341/api/meals/42/image`) constructed from the request's `Host` header.
3. Given a meal with id=42 exists and has no image, when the JSON-LD response is examined, then the `image` property is absent.
4. Given no meal with id=9999 exists, when `GET /api/meals/9999` is called with `Accept: application/ld+json`, then a 404 JSON error is returned (same as the existing plain-JSON endpoint).

### Scenario 2 - Export all meals as JSON-LD (P2)
A user wants to bulk-export their entire meal collection for backup or migration to another recipe manager.

**Acceptance**
1. Given 3 meals exist, when `GET /api/meals` is called with `Accept: application/ld+json`, then the response contains a JSON-LD `@graph` array with exactly 3 `Recipe` objects.
2. Given 0 meals exist, when `GET /api/meals` is called with `Accept: application/ld+json`, then the response contains `@graph: []` with zero recipe objects.
3. Each recipe object in the graph has the same fields as a single-meal response.

### Scenario 3 - Default behavior unchanged (P1)
Existing API consumers that don't request JSON-LD must see no change.

**Acceptance**
1. Given a meal exists, when `GET /api/meals/42` is called without `Accept: application/ld+json`, then the response is the existing plain JSON format (`Content-Type: application/json`), unchanged.
2. Given meals exist, when `GET /api/meals` is called without `Accept: application/ld+json`, then the response is the existing plain JSON array, unchanged.

## Functional Requirements
- **FR-001**: `GET /api/meals/:id` with `Accept: application/ld+json` returns the meal as a single schema.org `Recipe` JSON-LD object.
- **FR-002**: `GET /api/meals` with `Accept: application/ld+json` returns all meals as a JSON-LD document with `@graph` containing zero or more `Recipe` objects.
- **FR-003**: Field mapping: `name` → `name`, `ingredients` (each joined as `"{quantity} {name}"` or `"{name}"` when quantity is absent) → `recipeIngredient` string array, `instructions` → `recipeInstructions` text, `created_at` (ISO 8601) → `datePublished`, `updated_at` (ISO 8601) → `dateModified`.
- **FR-004**: When `has_image` is true, include an absolute `image` URL pointing to `/api/meals/:id/image` constructed from the request's `Host` header (default scheme `http`).
- **FR-005**: When `has_image` is false, omit the `image` property entirely.
- **FR-006**: Omit all schema.org Recipe properties that MealMe does not store: `cookTime`, `prepTime`, `totalTime`, `recipeYield`, `recipeCategory`, `recipeCuisine`, `nutrition`, `author`, `description`, `keywords`, `suitableForDiet`, `aggregateRating`, `video`, and any other properties not listed in FR-003 or FR-004.
- **FR-007**: Response `Content-Type` is `application/ld+json` when JSON-LD is requested.
- **FR-008**: Requests without `Accept: application/ld+json` continue to return the existing `application/json` responses unchanged.

## Key Entities
- **Recipe (JSON-LD output)**: A schema.org `Recipe` object with `@context: "https://schema.org"`, `@type: "Recipe"`, and the mapped properties from FR-003/FR-004.

## Edge Cases
- Meal with empty `instructions` → `recipeInstructions` emitted as `""`.
- Ingredient with quantity `null` or empty string → ingredient text is just the name, no quantity prefix.
- Ingredient with both quantity and name → text is `"{quantity} {name}"` (single space separator).
- Meal not found → 404 with existing JSON error body, not JSON-LD.
- Zero meals in database → `@graph` is an empty array `[]`.
- Request `Host` header missing or unparseable → omit `image` property (degraded output rather than error).

## Assumptions
- The `Host` header from the incoming request is sufficient to construct absolute image URLs; no configurable base URL is needed for v1.
- No authentication required (same as existing API — single-user local application).
- The existing `/api/meals/:id/image` endpoint that serves JPEG bytes is an acceptable image URL for schema.org consumers.
- `Accept` header-based content negotiation is the mechanism for selecting JSON-LD output; no `?format=` query parameter is added.

## Success Criteria
- **SC-001**: A schema.org validator (https://validator.schema.org) accepts the output of both single-meal and all-meals endpoints without errors.
- **SC-002**: The Google Rich Results Test accepts the JSON-LD as valid Recipe structured data (even if it flags missing optional fields as warnings, not errors).
- **SC-003**: Existing API endpoints and their response formats are unchanged when `Accept: application/ld+json` is not present.
- **SC-004**: All existing Rust tests (`cargo test`) continue to pass without modification.
- **SC-005**: A meal round-tripped through import (paste raw JSON-LD) and export produces semantically equivalent output for the fields MealMe stores.
