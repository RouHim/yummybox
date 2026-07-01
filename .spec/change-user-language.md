# Feature Specification: User-selectable language dropdown in the footer

**Created**: 2026-07-01
**Status**: Approved
**Input**: Let the user change UI language via a button in the head bar and store it in the browser; follow online best practices; use the taste skill for frontend design.

## Goal
Let users manually switch the UI language via a dropdown in the footer, showing flags and language names, and persist the choice in the browser so it survives reloads. Currently the app only auto-detects from `navigator.language` and writes nothing to storage. Supported locales stay `en` and `de`; a "System" option defers to navigator detection, preserving current auto-detect behavior on first visit.

## User Scenarios
### Scenario 1 - Switch to German as an English-locale user (P1)
A user whose browser is set to English wants the MealMe UI in German. They open the language dropdown in the footer and select "Deutsch". The entire UI re-renders in German immediately, `<html lang>` updates to `de`, and the choice persists across reloads.

**Acceptance**
1. Given the app is loaded with navigator.language `en-US` When the user opens the language dropdown and selects "Deutsch" Then all visible `t()` strings switch to German and `document.documentElement.lang` equals `de`.
2. Given the user selected "Deutsch" in a previous session When the page reloads Then the UI is in German and `localStorage.getItem('mealme-locale')` returns `de`.

### Scenario 2 - Switch to English as a German-locale user (P1)
A user whose browser is set to `de-DE` wants the UI in English. They select "English" from the dropdown.

**Acceptance**
1. Given the app is loaded with navigator.language `de-DE` When the user selects "English" from the dropdown Then all visible `t()` strings switch to English and `document.documentElement.lang` equals `en`.
2. Given the user selected "English" in a previous session When the page reloads Then the UI is in English and `localStorage.getItem('mealme-locale')` returns `en`.

### Scenario 3 - Return to system language via "System" option (P2)
A user who previously manually selected a language wants to return to automatic detection.

**Acceptance**
1. Given `localStorage.getItem('mealme-locale')` is `de` When the user selects "System" from the dropdown Then the locale resolves via `detectInitialLocale()` (based on `navigator.language`) and `localStorage.setItem('mealme-locale', 'system')` is called.
2. Given `localStorage.getItem('mealme-locale')` is `system` and navigator.language is `de-DE` When the page loads Then the UI is in German.

### Scenario 4 - First visit with no stored preference (P2)
A first-time visitor with no `mealme-locale` in localStorage sees the UI in their navigator-detected language.

**Acceptance**
1. Given `localStorage.getItem('mealme-locale')` returns `null` When the app loads Then `detectInitialLocale()` determines the language and `localStorage` is not written (no forced write on first visit).
2. Given navigator.language is `fr-FR` (unsupported) When the app loads with no stored preference Then the locale is `en` (fallback).

## Functional Requirements
- **FR-001**: A language dropdown control must render in the footer's left side, beside the existing photo attribution `<p class="attribution">`.
- **FR-002**: The dropdown must offer exactly three options: "System" (flag 🖥️), "English" (flag 🇬🇧), "Deutsch" (flag 🇩🇪). Each option shows a flag glyph followed by the localized language name.
- **FR-003**: Selecting an option must call `setLocale(value)` where `value` is `'en'`, `'de'`, or `'system'`, and must persist that value to `localStorage` under the key `mealme-locale`.
- **FR-004**: On app init, `initLocale()` must read `localStorage.getItem('mealme-locale')` first. If the value is `'en'` or `'de'`, use it directly. If the value is `'system'` or `null`, resolve via `detectInitialLocale()`. If storage is unavailable (`typeof localStorage === 'undefined'`), resolve via `detectInitialLocale()`. Invalid stored values fall back to `detectInitialLocale()`.
- **FR-005**: When the stored value is `'system'` or `null`, `initLocale()` must NOT write to localStorage (lazy write: only persist on explicit user selection).
- **FR-006**: The `<html lang>` attribute must update reactively whenever the locale changes (existing `$effect` in `+layout.svelte` already does `document.documentElement.lang = getLocale()`).
- **FR-007**: The dropdown trigger button must display the current effective locale's flag and name (e.g. 🇬🇧 English when `en`, 🇩🇪 Deutsch when `de`).
- **FR-008**: The dropdown must be keyboard accessible: trigger is focusable with visible focus ring (matching existing `.glass :focus-visible` outline), opens on Enter/Space/Click, navigable with Arrow keys, Escape closes it.
- **FR-009**: The dropdown must follow the `design-taste-frontend` skill directives: WCAG AA contrast on all interactive states, visible focus rings, no AI-tell patterns (no em-dashes in labels, no decorative status dots), `prefers-reduced-transparency` fallback if glassmorphism is used, real flag treatment via Unicode regional-indicator emoji (not hand-rolled SVG paths).
- **FR-010**: No new third-party i18n library may be added. The existing `web/src/lib/i18n/` module structure is extended.
- **FR-011**: The existing `Locale` type (`'en' | 'de'`) is extended to a preference type `'en' | 'de' | 'system'` for storage, while the runtime `_locale` state remains `'en' | 'de'` (since `system` resolves to a concrete locale).

## Key Entities
- **LocalePreference**: `'en' | 'de' | 'system'` — the value persisted in localStorage and selected in the dropdown. `system` means "defer to `detectInitialLocale()`".
- **Locale**: `'en' | 'de'` — the resolved runtime locale used by `t()`, `formatNumber()`, `formatDate()`.

## Edge Cases
- **Storage unavailable** (private mode, SSR): `initLocale()` must fall back to `detectInitialLocale()` without throwing. Mirrors the existing `theme.svelte.ts` pattern (`if (typeof localStorage === 'undefined') return ...`).
- **Invalid stored value** (e.g. `"fr"` or corrupted JSON): ignored, falls back to `detectInitialLocale()`. No write occurs.
- **`localStorage.setItem` throws** (quota exceeded): must not crash the app. Wrap in try/catch, matching `llm-config.svelte.ts` pattern.
- **Unsupported navigator.language** (e.g. `fr-FR`, `es-ES`): `detectInitialLocale()` already returns `'en'` as fallback.
- **Existing E2E tests to update**: Three tests in `tests/e2e/planner.spec.ts` assert `.lang-toggle` does not exist and `localStorage.getItem('mealme.locale')` is null. These must be updated: (a) the selector changes from `.lang-toggle` to the new dropdown's class, (b) the tests flip from "no toggle / not written" to asserting the dropdown exists and `system`/null behavior, (c) the localStorage key changes from `'mealme.locale'` to `'mealme-locale'`.

## Research Notes
- https://stackoverflow.com/questions/25154515/save-last-language-preference-of-the-user-in-a-cookie — localStorage is preferred over cookies for client-side-only preferences; cookies only needed when the server must read the value server-side (not applicable here: MealMe is an SPA with no SSR and no server-side locale logic).
- https://www.permit.io/blog/cookies-vs-local-storage (03/2025) — "Local storage is ideal for storing non-sensitive data on the client side" including user preferences such as language settings.
- The existing `web/src/lib/theme.svelte.ts` module already implements the exact localStorage persistence pattern (`mealme-theme` key, `readStored()` / `persist()` pair) and is the template to mirror for the locale module.
- The `design-taste-frontend` skill (loaded via `skill://design-taste-frontend`) Section 13 declares dense product UI / app chrome is out of scope for its landing-page focus, but its directives on icons (Section 3.C: no hand-rolled SVG paths), contrast (Section 4.5: WCAG AA), interactive states (empty/loading/error/tactile feedback), and AI-tell avoidance (Section 9: no em-dashes, no decorative dots) still bind the dropdown component.

## Assumptions
- Emoji flags (🖥️ 🇬🇧 🇩🇪) are acceptable for the flag glyphs. No SVG flag assets exist in `web/static/` or `web/src/lib/assets/`, and the project uses a hand-rolled `Icon.svelte` rather than a third-party icon library the taste skill would prefer. For flags specifically, Unicode regional-indicator emoji is the standard web approach and avoids hand-rolled SVG paths that the skill prohibits.
- "System" resolves on each page load via the existing `detectInitialLocale()` function; it does not freeze the navigator language at selection time.
- The default selection (first visit, nothing stored) is effectively `system` — `initLocale()` calls `detectInitialLocale()` and does not write to localStorage, matching the current behavior.
- The existing `detectInitialLocale()` function and `t()` / `formatNumber()` / `formatDate()` internals remain unchanged in their core logic; only `initLocale()` gains the localStorage-read step and a new `setLocale()` function is added for persistence.
- The locale dropdown lives only in `+layout.svelte` (the footer). No per-page language buttons.

## Success Criteria
- **SC-001**: Running `cd web && npm test` passes, including updated i18n tests covering `setLocale()`, `initLocale()` with a stored `mealme-locale` value, and the `system` fallback path.
- **SC-002**: Running `cd web && npm run check` (svelte-check / TypeScript strict) passes with the new `LocalePreference` type and extended `initLocale`/`setLocale` signatures.
- **SC-003**: Running `cd tests && npm test` passes, including updated planner E2E tests that assert the dropdown exists and the `mealme-locale` localStorage key behaves correctly (system → not written, explicit selection → written).
- **SC-004**: Running `cd web && npm run test:e2e` passes, including the visual/styling ambient-background suite (regression check that footer layout is not broken).
- **SC-005**: Manual check — open the app, select "Deutsch" from the footer dropdown, verify all `t()` strings switch to German, reload, verify German persists, then select "System" and verify it returns to the navigator-detected language.
