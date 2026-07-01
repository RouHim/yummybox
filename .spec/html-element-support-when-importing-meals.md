# Feature Specification: Safe HTML Rendering in Imported Meal Instructions

**Created**: 2026-06-29
**Status**: Approved
**Input**: HTML element support when importing meals (e.g., from HelloFresh) — raw `<p dir=ltr>` tags should be sanitized and rendered instead of displayed as escaped text.

## Goal
When meals are imported from recipe websites, the instructions field often contains HTML markup (e.g., `<p dir=ltr>...</p>` from HelloFresh's JSON-LD `recipeInstructions`). Currently this HTML is stored verbatim and displayed as escaped text in the UI, making instructions unreadable. The fix sanitizes imported instructions to a safe subset of HTML tags and renders them in the cooking view so users see clean, formatted text.

## User Scenarios

### Scenario 1 - Import from URL with HTML instructions (P1)
A user imports a recipe from HelloFresh via the "Import from URL" feature. The source page's JSON-LD contains instructions wrapped in `<p dir=ltr>` tags. After import, the cooking view shows clean paragraphs instead of raw HTML tags.

**Acceptance**
1. Given a HelloFresh recipe URL whose JSON-LD instructions contain `<p dir=ltr>Step 1</p><p dir=ltr>Step 2</p>`, when the user imports via `/import/url`, then the returned draft contains sanitized instructions `<p>Step 1</p><p>Step 2</p>` with no attributes.
2. Given an imported meal with sanitized HTML instructions, when the user opens the cooking view (`/meals/:id`), then instructions render as formatted paragraphs (not escaped text).

### Scenario 2 - Import from paste with HTML instructions (P2)
A user pastes raw HTML containing a schema.org Recipe. The extracted instructions may contain HTML markup. The same sanitization applies.

**Acceptance**
1. Given pasted HTML whose JSON-LD instructions contain `<strong>important</strong>` and `<br>` tags, when the user imports via `/import/paste`, then the returned draft preserves `<strong>` and `<br>` but strips any attributes and non-whitelisted tags.

### Scenario 3 - Import via LLM (P2)
A user imports a recipe via LLM (image or text hint). The LLM rarely returns HTML, but sanitization is applied as defense-in-depth.

**Acceptance**
1. Given an LLM import that returns plain text instructions, when sanitization runs, then the text passes through unchanged.
2. Given an LLM import that unexpectedly returns HTML in instructions, when sanitization runs, then only whitelisted tags survive.

### Scenario 4 - Edit an imported meal (P2)
A user opens the edit form for an imported meal. The textarea shows sanitized HTML markup (e.g., `<p>Step 1</p>`). The user can freely edit the text. On save, the edited text is stored as-is (no re-sanitization — the user controls the content).

**Acceptance**
1. Given an imported meal with sanitized HTML instructions, when the user opens the edit modal, then the instructions textarea contains the sanitized HTML markup verbatim.
2. Given the user edits the instructions textarea and saves, then the stored instructions reflect the user's edits exactly.

### Scenario 5 - Manually created meal (P2)
A user manually creates a meal with plain text instructions (no HTML). The cooking view renders the text with preserved newlines.

**Acceptance**
1. Given a manually created meal with instructions "Step 1\nStep 2\nStep 3", when the user opens the cooking view, then the text renders with line breaks preserved via `white-space: pre-wrap`.

## Functional Requirements

- **FR-001**: The server MUST sanitize HTML in the `instructions` field during all three import operations (`/import/url`, `/import/paste`, `/import/llm`) before returning the `ImportDraft`.
- **FR-002**: Sanitization MUST use a whitelist-based HTML sanitizer (the `ammonia` crate).
- **FR-003**: The whitelist MUST allow only these tags: `<p>`, `<br>`, `<strong>`, `<em>`, `<b>`, `<i>`, `<ul>`, `<ol>`, `<li>`.
- **FR-004**: All HTML attributes MUST be stripped from every tag.
- **FR-005**: Sanitization MUST run after `recipe_scraper` extraction but before the `ImportDraft` is returned to the frontend.
- **FR-006**: The sanitization function MUST be a pure helper in `src/recipe.rs`.
- **FR-007**: The cooking view (`web/src/routes/meals/[id]/+page.svelte`) MUST render instructions using Svelte's `{@html}` directive.
- **FR-008**: The instructions container in the cooking view MUST be a `<div>` (not `<p>`) to allow block-level HTML children.
- **FR-009**: The instructions container MUST retain `white-space: pre-wrap` CSS so plain-text instructions preserve manual newlines.
- **FR-010**: Manual create and update meal endpoints (`POST /api/meals`, `PUT /api/meals/:id`) MUST NOT sanitize HTML — only import paths apply sanitization.
- **FR-011**: Empty string or whitespace-only instructions after sanitization MUST be treated as an empty instructions string.

## Edge Cases

- **All content is non-whitelisted tags**: `<script>alert(1)</script>` sanitizes to empty string — treated as empty instructions.
- **Deeply nested tags**: `<p><strong><em>text</em></strong></p>` — ammonia handles nesting; all three tags preserved, attributes stripped.
- **HTML entities**: `&amp;`, `&uuml;`, `&#39;` — ammonia preserves them; browser decodes them correctly in `{@html}` context.
- **Existing meals with raw HTML**: Meals imported before this change still contain unsanitized HTML. They will display as escaped text in the cooking view (Svelte's default escaping). Users can re-import or manually edit to clean them up. No database migration.
- **Instructions exceed 20000 characters after sanitization**: The existing validation limit in `db::validate_meal` still applies. If sanitization produces text > 20000 chars, the create/update call will reject it (existing behavior).
- **Self-closing tags**: `<br/>` and `<br>` are both handled by ammonia; output normalized to `<br>`.
- **Whitespace between tags**: `<p>  text  </p>` — ammonia preserves inner whitespace; `white-space: pre-wrap` handles it consistently.

## Research Notes

- **ammonia crate** (v4.1.2, MIT, MSRV 1.80) — https://crates.io/crates/ammonia — the standard Rust HTML sanitizer used by crates.io and docs.rs. Whitelist-based, strips unknown tags and all attributes by default.
- **schema.org recipeInstructions** — https://schema.org/Recipe — the spec allows HTML in text fields. Many recipe sites (HelloFresh, Chefkoch) emit formatted HTML in JSON-LD instructions. The `recipe_scraper` crate extracts these verbatim.
- **Svelte `{@html}`** — https://svelte.dev/docs/svelte/@html — the correct directive for rendering pre-sanitized HTML. Combined with `white-space: pre-wrap` on the container, it handles both plain text and HTML content.

## Assumptions

- Server-side sanitization is sufficient for a local single-user application. No client-side sanitizer (e.g., DOMPurify) is needed.
- Existing meals with unsanitized HTML are acceptable as-is; a database migration is out of scope.
- Users accept seeing sanitized HTML markup in the edit textarea for imported meals — this matches today's behavior where raw tags are already visible, but the tags will be cleaner (no attributes, only whitelisted elements).
- The `recipe_scraper` crate will not be modified; HTML cleaning is a post-processing step in `recipe.rs`.

## Success Criteria

- **SC-001**: Importing the HelloFresh URL from the brief produces instructions with no `dir` attribute and no non-whitelisted tags.
- **SC-002**: The cooking view renders imported instructions with visible paragraph breaks (not raw `<p>` text).
- **SC-003**: Importing a recipe with `<script>alert(1)</script>` in instructions produces instructions without the `<script>` tag.
- **SC-004**: Manually created meals with plain text instructions render identically before and after this change.
- **SC-005**: All existing tests pass after the change (Rust `cargo test`, frontend `npm test`, and E2E suites).
- **SC-006**: Editing an imported meal and saving preserves the user's edits exactly (no double-sanitization).
